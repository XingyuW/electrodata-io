use super::{DetectedLayout, Detection, FormatHandler, FormatId, numeric, trim_cell};
use crate::detection::normalize_for_match;
use crate::{ColumnDescriptor, ColumnRole, HeaderPolicy, Probe, ReadOptions, Unit};
use std::collections::HashMap;

/// Compatibility format id backed by the generalized N-column table handler.
pub(crate) struct RegularXyHandler;

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
        let nonempty: Vec<(usize, &[String])> = probe
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.iter().any(|cell| !trim_cell(cell).is_empty()))
            .map(|(index, row)| (index, row.as_slice()))
            .take(8)
            .collect();
        let (first_index, first) = *nonempty.first()?;
        if first.len() < 2 {
            return None;
        }
        let first_numeric = first.iter().all(|cell| numeric(cell));
        let has_header = match options.header_policy {
            HeaderPolicy::Auto => !first_numeric,
            HeaderPolicy::Present => true,
            HeaderPolicy::Absent => false,
        };
        if has_header {
            let roles: Vec<ColumnRole> = first.iter().map(|cell| header_role(cell)).collect();
            if roles.contains(&ColumnRole::Frequency)
                && roles.iter().any(|role| {
                    matches!(
                        role,
                        ColumnRole::ImpedanceReal
                            | ColumnRole::ImpedanceImaginary
                            | ColumnRole::ImpedanceMagnitude
                            | ColumnRole::ImpedancePhase
                    )
                })
            {
                return None;
            }
        }
        let width = first.len();
        let data_position = usize::from(has_header);
        let candidate_rows: Vec<&&[String]> = nonempty
            .iter()
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
        let data_start_row = nonempty.get(data_position)?.0;
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
            } else {
                0.60
            },
            evidence: vec![
                format!("{width} consistently shaped delimited columns"),
                if has_header {
                    "text header followed by numeric rows".into()
                } else {
                    "first row is numeric data".into()
                },
            ],
            layout: DetectedLayout {
                header_row: has_header.then_some(first_index),
                data_start_row,
                metadata_range: None,
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

pub(crate) fn header_role(value: &str) -> ColumnRole {
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
    } else if matches!(stem, "freq" | "frequency") {
        ColumnRole::Frequency
    } else if matches!(stem, "z'" | "zreal" | "re(z)" | "realz") {
        ColumnRole::ImpedanceReal
    } else if matches!(stem, "z\"" | "z''" | "zimag" | "im(z)" | "imaginaryz") {
        ColumnRole::ImpedanceImaginary
    } else if matches!(stem, "z" | "|z|" | "magnitude" | "zmod") {
        ColumnRole::ImpedanceMagnitude
    } else if matches!(stem, "phase" | "theta" | "φ") {
        ColumnRole::ImpedancePhase
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
