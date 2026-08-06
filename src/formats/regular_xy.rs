use super::{DetectedLayout, Detection, FormatHandler, FormatId, numeric, trim_cell};
use crate::detection::normalize_for_match;
use crate::{
    ColumnDescriptor, ColumnRole, ContainerFormat, HeaderPolicy, Probe, ReadOptions, Unit,
};
use std::collections::HashMap;

/// Compatibility format id backed by the generalized N-column table handler.
pub(crate) struct RegularXyHandler;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FirstRowClassification {
    Header(&'static str),
    Data(&'static str),
    Ambiguous(&'static str),
}

pub(crate) fn explicit_comment_row(row: &[String]) -> bool {
    row.first().is_some_and(|cell| {
        let value = trim_cell(cell);
        value.starts_with('#') || value.starts_with("//")
    })
}

pub(crate) fn classify_first_row(row: &[String]) -> FirstRowClassification {
    let mut numeric_cells = 0;
    let mut missing_cells = 0;
    let mut text_cells = 0;
    for cell in row {
        let value = trim_cell(cell);
        if value.is_empty() || missing_token(value) {
            missing_cells += 1;
        } else if numeric(value) {
            numeric_cells += 1;
        } else {
            text_cells += 1;
        }
    }
    match (numeric_cells, missing_cells, text_cells) {
        (numeric, _, 0) if numeric > 0 => {
            FirstRowClassification::Data("every non-missing cell is a parseable numeric token")
        }
        (0, 0, text) if text > 0 => {
            FirstRowClassification::Header("every cell is a non-numeric header token")
        }
        _ => FirstRowClassification::Ambiguous(
            "the row mixes numeric, missing, and/or non-numeric tokens",
        ),
    }
}

impl FormatHandler for RegularXyHandler {
    fn format_id(&self) -> FormatId {
        FormatId::RegularXy
    }

    fn detect(&self, probe: &Probe, options: &ReadOptions) -> Option<Detection> {
        if probe.rows.iter().take(20).flatten().any(|cell| {
            let value = normalize_for_match(cell);
            value.starts_with("file:")
                || value.starts_with("data source:")
                || value.starts_with("instrument model:")
                || value.contains("open circuit potential")
        }) {
            return None;
        }
        // Workbooks often have free-form acquisition notes before the actual
        // table. Search the bounded worksheet preview for a semantic time
        // header with compatible numeric rows rather than assuming a row
        // count. Delimited-text preambles retain their stricter, explicit
        // comment syntax behavior.
        let search_limit = if probe.container == ContainerFormat::Csv {
            8
        } else {
            options.probe_rows
        };
        let table_rows: Vec<(usize, &[String])> = probe
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.iter().any(|cell| !trim_cell(cell).is_empty()) && !explicit_comment_row(row)
            })
            .map(|(index, row)| (index, row.as_slice()))
            .take(search_limit)
            .collect();
        let first_candidate = if probe.container == ContainerFormat::Csv {
            Some(0)
        } else {
            table_rows
                .iter()
                .position(|(_, row)| {
                    row.len() >= 2
                        && matches!(classify_first_row(row), FirstRowClassification::Header(_))
                        && matches!(header_role(&row[0]), ColumnRole::Time)
                        && numeric_rows_after(&table_rows, row, 1) > 0
                })
                // Preserve established header-at-first-row generic-table
                // support. Preambles cannot take this path because they lack
                // immediate shaped numeric evidence.
                .or_else(|| {
                    table_rows.first().and_then(|(index, row)| {
                        (*index == 0
                            && row.len() >= 2
                            && matches!(classify_first_row(row), FirstRowClassification::Header(_))
                            && numeric_rows_after(&table_rows, row, 1) > 0)
                            .then_some(0)
                    })
                })
        };
        let (first_index, first) = *table_rows.get(first_candidate?)?;
        if first.len() < 2 {
            return None;
        }
        let classification = classify_first_row(first);
        let has_header = match options.header_policy {
            HeaderPolicy::Auto => !matches!(classification, FirstRowClassification::Data(_)),
            HeaderPolicy::Present => true,
            HeaderPolicy::Absent => false,
        };
        if has_header && eis_schema_candidate(first) {
            return None;
        }
        let width = first.len();
        let data_position = usize::from(has_header);
        let candidate_rows: Vec<&&[String]> = table_rows
            .iter()
            .skip(first_candidate?)
            .skip(data_position)
            .take(5)
            .map(|(_, row)| row)
            .collect();
        let shaped_rows = candidate_rows
            .iter()
            .filter(|row| row.len() == width)
            .count();
        let numeric_rows = candidate_rows
            .iter()
            .filter(|row| {
                row.len() == width
                    && row.iter().all(|cell| {
                        let value = trim_cell(cell);
                        value.is_empty() || missing_token(value) || numeric(value)
                    })
            })
            .count();
        if candidate_rows.is_empty() || shaped_rows == 0 || numeric_rows == 0 {
            return None;
        }
        let data_start_row = table_rows.get(first_candidate? + data_position)?.0;
        let semantic_header = has_header
            && first
                .iter()
                .any(|cell| !matches!(header_role(cell), ColumnRole::Unknown));
        Some(Detection {
            format: FormatId::RegularXy,
            confidence: if numeric_rows >= 3 && semantic_header {
                0.90
            } else if numeric_rows >= 3 {
                0.86
            } else if shaped_rows >= 2 {
                0.78
            } else if candidate_rows.len() > 1 {
                0.70
            } else if !has_header {
                0.65
            } else {
                0.60
            },
            evidence: vec![
                format!("{width} consistently shaped delimited columns"),
                if first_index > 0 {
                    format!(
                        "semantic time header discovered after {first_index} worksheet preamble row(s)"
                    )
                } else {
                    String::new()
                },
                if has_header {
                    "text header followed by numeric rows".into()
                } else {
                    "first row is numeric data".into()
                },
            ]
            .into_iter()
            .filter(|e| !e.is_empty())
            .collect(),
            layout: DetectedLayout {
                header_row: has_header.then_some(first_index),
                data_start_row,
                metadata_range: (first_index > 0).then_some(0..first_index),
                expected_column_count: width,
                has_header,
            },
        })
    }

    fn columns(
        &self,
        probe: &Probe,
        detection: &Detection,
    ) -> crate::Result<Vec<ColumnDescriptor>> {
        let source = detection.layout.header_row.and_then(|row| probe.row(row));
        let width = detection.layout.expected_column_count;
        let mut names = HashMap::<String, usize>::new();
        Ok((0..width)
            .map(|index| {
                let source_name = source
                    .and_then(|row| row.get(index))
                    .map(|value| trim_cell(value).to_owned())
                    .unwrap_or_default();
                let role = if source.is_none() {
                    if index == 0 {
                        ColumnRole::Time
                    } else {
                        ColumnRole::Unknown
                    }
                } else {
                    header_role(&source_name)
                };
                let unit = source
                    .and_then(|_| header_unit(&source_name))
                    .unwrap_or(Unit::Unknown);
                let base = if source.is_none() {
                    format!("column_{}", index + 1)
                } else {
                    canonical_name(&source_name, &role, &unit, index)
                };
                let count = names.entry(base.clone()).or_default();
                *count += 1;
                let canonical_name = if *count == 1 {
                    base
                } else {
                    format!("{base}_{}", *count)
                };
                ColumnDescriptor {
                    source_name,
                    canonical_name,
                    role,
                    unit: Some(unit),
                }
            })
            .collect())
    }
}

fn numeric_rows_after(
    table_rows: &[(usize, &[String])],
    header: &[String],
    minimum: usize,
) -> usize {
    table_rows
        .iter()
        .skip_while(|(_, row)| !std::ptr::eq(*row, header))
        .skip(1)
        .take(5)
        .filter(|(_, row)| {
            row.len() == header.len()
                && row.iter().all(|cell| {
                    let value = trim_cell(cell);
                    value.is_empty() || missing_token(value) || numeric(value)
                })
        })
        .take(minimum)
        .count()
}

pub(crate) fn header_role(value: &str) -> ColumnRole {
    if let [role] = eis_header_candidates(value).as_slice() {
        return role.clone();
    }
    let normalized = normalize_for_match(value);
    let compact: String = normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let stem = compact
        .split_once('/')
        .map(|(stem, _)| stem)
        .unwrap_or(&compact);
    if matches!(stem, "time" | "timestamp") || stem.starts_with("time(") {
        ColumnRole::Time
    } else if matches!(stem, "potential" | "voltage" | "ewe" | "e") {
        ColumnRole::Potential
    } else if matches!(stem, "current" | "i") {
        ColumnRole::Current
    } else if let Some(channel) = measurement_channel(stem) {
        ColumnRole::MeasurementChannel(channel)
    } else if stem == "x" {
        ColumnRole::X
    } else if stem == "y" {
        ColumnRole::Y
    } else {
        ColumnRole::Unknown
    }
}

/// Exact scientific EIS header meanings recognized without positional inference.
/// A multi-role result is deliberately retained for structured EIS schema errors.
pub(crate) fn eis_header_candidates(value: &str) -> Vec<ColumnRole> {
    let normalized = normalize_for_match(value);
    let semantic = normalized
        .split_once('/')
        .map(|(stem, _)| stem)
        .unwrap_or(&normalized)
        .trim();
    if semantic.contains('+') {
        return semantic.split('+').filter_map(eis_role_alias).collect();
    }
    eis_role_alias(semantic).into_iter().collect()
}

fn eis_role_alias(value: &str) -> Option<ColumnRole> {
    let compact: String = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    match compact.as_str() {
        "freq" | "frequency" => Some(ColumnRole::Frequency),
        "z'" | "zreal" | "zre" | "re(z)" | "realz" | "realimpedance" => {
            Some(ColumnRole::ImpedanceReal)
        }
        "z\"" | "z''" | "zim" | "zimag" | "zimaginary" | "im(z)" | "imaginaryz"
        | "imaginaryimpedance" => Some(ColumnRole::ImpedanceImaginary),
        "z" | "|z|" | "magnitude" | "zmod" | "impedancemagnitude" => {
            Some(ColumnRole::ImpedanceMagnitude)
        }
        "phase" | "phaseangle" | "theta" | "φ" => Some(ColumnRole::ImpedancePhase),
        _ => None,
    }
}

/// True when a header has enough independent EIS-role evidence to warrant EIS
/// schema validation, including an incomplete schema that must report an error.
pub(crate) fn eis_schema_candidate(row: &[String]) -> bool {
    if row.len() < 3 {
        return false;
    }
    let candidates: Vec<ColumnRole> = row
        .iter()
        .flat_map(|value| eis_header_candidates(value))
        .collect();
    [
        ColumnRole::Frequency,
        ColumnRole::ImpedanceReal,
        ColumnRole::ImpedanceImaginary,
    ]
    .iter()
    .filter(|role| candidates.contains(role))
    .count()
        >= 2
}

pub(crate) fn measurement_channel(stem: &str) -> Option<u32> {
    let digits = stem.strip_prefix("ch").or_else(|| stem.strip_prefix('e'))?;
    let parsed = digits.parse::<u32>().ok()?;
    (parsed > 0).then_some(parsed)
}

pub(crate) fn header_unit(value: &str) -> Option<Unit> {
    let raw = trim_cell(value)
        .rsplit_once('/')
        .map(|(_, unit)| unit.trim())
        .or_else(|| {
            trim_cell(value)
                .rsplit_once('(')
                .and_then(|(_, tail)| tail.strip_suffix(')'))
                .map(str::trim)
        })?;
    let normalized = normalize_for_match(raw);
    Some(match normalized.as_str() {
        "s" | "sec" | "second" | "seconds" => Unit::Second,
        "h" | "hr" | "hour" | "hours" => Unit::Hour,
        "d" | "day" | "days" => Unit::Day,
        "hz" => Unit::Hertz,
        "ohm" | "ohms" | "ω" => Unit::Ohm,
        "deg" | "degree" | "degrees" | "°" => Unit::Degree,
        "v" | "volt" | "volts" => Unit::Volt,
        "mv" => Unit::Millivolt,
        "a" | "amp" | "amps" => Unit::Ampere,
        "ma" => Unit::Milliampere,
        "ua" | "µa" | "μa" => Unit::Microampere,
        _ => Unit::Other(raw.to_owned()),
    })
}

fn canonical_name(source: &str, role: &ColumnRole, unit: &Unit, index: usize) -> String {
    match role {
        ColumnRole::Time => unit_suffix("time", unit),
        ColumnRole::Frequency => unit_suffix("frequency", unit),
        ColumnRole::ImpedanceReal => unit_suffix("z_real", unit),
        ColumnRole::ImpedanceImaginary => unit_suffix("z_imag", unit),
        ColumnRole::ImpedanceMagnitude => unit_suffix("z_magnitude", unit),
        ColumnRole::ImpedancePhase | ColumnRole::Phase => unit_suffix("phase", unit),
        ColumnRole::Potential => unit_suffix("potential", unit),
        ColumnRole::Current => unit_suffix("current", unit),
        ColumnRole::MeasurementChannel(channel) | ColumnRole::PotentialChannel(channel) => {
            unit_suffix(&format!("ch{channel}"), unit)
        }
        ColumnRole::X => "x".into(),
        ColumnRole::Y => "y".into(),
        ColumnRole::Unknown | ColumnRole::Custom(_) => {
            let name: String = normalize_for_match(source)
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .trim_matches('_')
                .to_owned();
            if name.is_empty() {
                format!("column_{}", index + 1)
            } else {
                name
            }
        }
    }
}

fn unit_suffix(stem: &str, unit: &Unit) -> String {
    let suffix = match unit {
        Unit::Second => "s",
        Unit::Hour => "h",
        Unit::Day => "d",
        Unit::Hertz => "hz",
        Unit::Ohm => "ohm",
        Unit::Degree => "deg",
        Unit::Volt => "v",
        Unit::Millivolt => "mv",
        Unit::Ampere => "a",
        Unit::Milliampere => "ma",
        Unit::Microampere => "ua",
        Unit::Unknown | Unit::Other(_) => return stem.into(),
    };
    format!("{stem}_{suffix}")
}

fn missing_token(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "na" | "n/a" | "nan" | "null" | "missing"
    )
}
