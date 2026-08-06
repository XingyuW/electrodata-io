use super::regular_xy::{eis_header_candidates, eis_schema_candidate, header_unit};
use super::{DetectedLayout, Detection, FormatHandler, FormatId, trim_cell};
use crate::detection::normalize_for_match;
use crate::{
    ColumnDescriptor, ColumnRole, Diagnostic, DiagnosticCode, EisSchemaError, Error, Probe,
    ReadOptions, Severity, Unit,
};
use std::path::Path;

pub(crate) struct ChiOcptHandler;
pub(crate) struct ChiMultichannelOcptHandler;
pub(crate) struct ChiEisHandler;

/// EIS selection has different evidence requirements when the caller already
/// selected the format than when the reader is classifying an unknown table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EisDispatchMode {
    Automatic,
    Forced,
}

fn header_key(value: &str) -> String {
    normalize_for_match(trim_cell(value))
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn technique_row(
    probe: &Probe,
    matcher: impl Fn(&str) -> Option<&'static str>,
) -> Option<(usize, String)> {
    probe.rows.iter().enumerate().find_map(|(row_index, row)| {
        row.iter().find_map(|cell| {
            let normalized = normalize_for_match(cell);
            matcher(&normalized).map(|pattern| (row_index, pattern.to_owned()))
        })
    })
}

fn header_and_data(probe: &Probe, predicate: impl Fn(&[String]) -> bool) -> Option<(usize, usize)> {
    let header = probe.rows.iter().position(|row| predicate(row))?;
    let expected = probe.row(header)?.len();
    let following: Vec<(usize, &[String])> = probe
        .rows
        .iter()
        .enumerate()
        .skip(header + 1)
        .filter(|(_, row)| row.iter().any(|cell| !trim_cell(cell).is_empty()))
        .map(|(index, row)| (index, row.as_slice()))
        .take(3)
        .collect();
    let (data, first) = *following.first()?;
    if !numeric_row(first, expected) {
        return None;
    }
    if following.len() > 1 && !following.iter().all(|(_, row)| numeric_row(row, expected)) {
        return None;
    }
    Some((header, data))
}

/// Finds an EIS header and its first non-empty row without requiring the row to
/// be numerically valid or rectangular. EIS schema evidence is strong enough to
/// select the handler; the structured decoder must retain malformed-cell and
/// ragged-row context instead of turning either condition into a detection miss.
fn eis_header_and_data(
    probe: &Probe,
    predicate: impl Fn(&[String]) -> bool,
) -> Option<(usize, usize)> {
    let header = probe.rows.iter().position(|row| predicate(row))?;
    let data = probe
        .rows
        .iter()
        .enumerate()
        .skip(header + 1)
        .find(|(_, row)| row.iter().any(|cell| !trim_cell(cell).is_empty()))?
        .0;
    Some((header, data))
}

/// Selects the source header for an explicitly requested EIS read without
/// applying automatic-detection confidence or completeness requirements.
///
/// A row containing any recognized EIS role wins. When no semantic role is
/// recognized, the first multi-column text row is treated as the header so
/// `validate_eis_schema` can report the deterministic first missing role with
/// an empty detected-role collection and the original source headers.
fn forced_eis_header_and_data(probe: &Probe) -> Option<(usize, usize)> {
    let header = probe
        .rows
        .iter()
        .position(|row| {
            row.iter()
                .any(|value| !eis_header_candidates(value).is_empty())
        })
        .or_else(|| {
            probe.rows.iter().position(|row| {
                row.len() >= 2
                    && row.iter().all(|cell| {
                        let value = trim_cell(cell);
                        !value.is_empty() && value.parse::<f64>().is_err()
                    })
            })
        })?;
    let data = probe
        .rows
        .iter()
        .enumerate()
        .skip(header + 1)
        .find(|(_, row)| row.iter().any(|cell| !trim_cell(cell).is_empty()))
        .map_or(header + 1, |(index, _)| index);
    Some((header, data))
}

fn numeric_row(row: &[String], expected: usize) -> bool {
    row.len() == expected
        && row.iter().all(|cell| {
            let cell = trim_cell(cell);
            cell.is_empty() || cell.parse::<f64>().is_ok()
        })
}

fn is_time(value: &str) -> bool {
    matches!(
        header_key(value).as_str(),
        "time/s" | "time/sec" | "time/second" | "time(seconds)" | "time"
    )
}

fn is_potential(value: &str) -> bool {
    matches!(
        header_key(value).as_str(),
        "potential/v" | "potential(volt)" | "potential" | "voltage/v" | "voltage"
    )
}

fn channel_number(value: &str) -> Option<u32> {
    let compact = header_key(value);
    let stem = compact.strip_suffix("/v").unwrap_or(&compact);
    let digits = stem.strip_prefix("ch").or_else(|| stem.strip_prefix('e'))?;
    let parsed = digits.parse::<u32>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn is_potential_column(value: &str) -> bool {
    is_potential(value) || channel_number(value).is_some()
}

fn ocpt_header(row: &[String]) -> bool {
    row.len() >= 2 && row.first().is_some_and(|cell| is_time(cell))
}

impl FormatHandler for ChiOcptHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiOcpt
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        let (_, technique) = technique_row(probe, ocpt_technique)?;
        let (header, data) = header_and_data(probe, |row| {
            ocpt_header(row)
                && row.len() == 2
                && row
                    .get(1)
                    .is_some_and(|value| is_potential(value) || channel_number(value).is_some())
        })?;
        Some(Detection {
            format: FormatId::ChiOcpt,
            confidence: ocpt_confidence(&technique),
            evidence: vec![
                format!("OCPT technique signature: {technique}"),
                "two-column time and potential header roles".into(),
                format!("matched source header: {}", probe.row(header)?.join(", ")),
            ],
            layout: DetectedLayout {
                header_row: Some(header),
                data_start_row: data,
                metadata_range: Some(0..header),
                expected_column_count: 2,
                has_header: true,
            },
        })
    }
    fn columns(
        &self,
        probe: &Probe,
        detection: &Detection,
    ) -> crate::Result<Vec<ColumnDescriptor>> {
        let row = probe
            .row(detection.layout.header_row.ok_or(Error::HeaderNotFound)?)
            .ok_or(Error::HeaderNotFound)?;
        let potential_role = channel_number(&row[1])
            .map(ColumnRole::MeasurementChannel)
            .unwrap_or(ColumnRole::Potential);
        let time_unit = header_unit(&row[0]).unwrap_or(Unit::Unknown);
        let potential_unit = header_unit(&row[1]).unwrap_or(Unit::Unknown);
        let time_name = canonical_name("time", &time_unit);
        let potential_name = canonical_name("potential", &potential_unit);
        Ok(vec![
            descriptor(&row[0], &time_name, ColumnRole::Time, time_unit),
            descriptor(&row[1], &potential_name, potential_role, potential_unit),
        ])
    }
}

impl FormatHandler for ChiMultichannelOcptHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiMultichannelOcpt
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        let (_, technique) = technique_row(probe, ocpt_technique)?;
        let (header, data) = header_and_data(probe, |row| {
            row.len() > 2
                && row.first().is_some_and(|v| is_time(v))
                && row.iter().skip(1).all(|v| is_potential_column(v))
                && row.iter().skip(1).filter(|v| is_potential(v)).count() <= 1
                && row.iter().skip(1).count() >= 2
        })?;
        let width = probe.row(header)?.len();
        Some(Detection {
            format: FormatId::ChiMultichannelOcpt,
            confidence: ocpt_confidence(&technique) + 0.005,
            evidence: vec![
                format!("OCPT technique signature: {technique}"),
                "time header and two or more compatible potential-channel headers".into(),
                format!("matched source header: {}", probe.row(header)?.join(", ")),
            ],
            layout: DetectedLayout {
                header_row: Some(header),
                data_start_row: data,
                metadata_range: Some(0..header),
                expected_column_count: width,
                has_header: true,
            },
        })
    }
    fn columns(
        &self,
        probe: &Probe,
        detection: &Detection,
    ) -> crate::Result<Vec<ColumnDescriptor>> {
        let row = probe
            .row(detection.layout.header_row.ok_or(Error::HeaderNotFound)?)
            .ok_or(Error::HeaderNotFound)?;
        let time_unit = header_unit(&row[0]).unwrap_or(Unit::Unknown);
        let mut result = vec![descriptor(
            &row[0],
            &canonical_name("time", &time_unit),
            ColumnRole::Time,
            time_unit,
        )];
        for value in row.iter().skip(1) {
            let unit = header_unit(value).unwrap_or(Unit::Unknown);
            if let Some(channel) = channel_number(value) {
                result.push(descriptor(
                    value,
                    &canonical_name(&format!("ch{channel}"), &unit),
                    ColumnRole::MeasurementChannel(channel),
                    unit,
                ));
            } else if is_potential(value) {
                result.push(descriptor(
                    value,
                    &canonical_name("potential", &unit),
                    ColumnRole::Potential,
                    unit,
                ));
            } else {
                return Err(Error::InvalidColumnSignature(value.clone()));
            }
        }
        Ok(result)
    }
}

pub(crate) fn validate_eis_schema(
    path: &Path,
    probe: &Probe,
    detection: &Detection,
    options: &ReadOptions,
) -> crate::Result<()> {
    let header_index = detection.layout.header_row.ok_or(Error::HeaderNotFound)?;
    let header = probe.row(header_index).ok_or(Error::HeaderNotFound)?;
    let format_candidate = detection.format.clone();
    let context = |diagnostic: Diagnostic| {
        diagnostic.with_context(
            path,
            probe.worksheet_name.as_deref(),
            &format_candidate,
            options.effective_profile(),
        )
    };

    let mut resolved = Vec::with_capacity(header.len());
    for (index, source) in header.iter().enumerate() {
        let candidates = eis_header_candidates(source);
        if candidates.len() > 1 {
            let diagnostic = context(
                Diagnostic::new(
                    Severity::Error,
                    DiagnosticCode::EisSchema,
                    "EIS header matches incompatible scientific roles; no positional resolution was applied",
                )
                .at(header_index + 1, Some(index + 1))
                .for_column(trim_cell(source))
                .with_candidate_roles(candidates.clone())
                .with_resolution("rejected: ambiguous EIS header"),
            );
            return Err(Error::EisSchemaConflict {
                details: Box::new(EisSchemaError {
                    path: path.to_path_buf(),
                    format_candidate,
                    row: header_index + 1,
                    column: index + 1,
                    header: trim_cell(source).to_owned(),
                    candidate_roles: candidates,
                    conflicting_role: None,
                    diagnostics: vec![diagnostic],
                }),
            });
        }
        resolved.push(candidates.into_iter().next());
    }

    for role in [
        ColumnRole::Frequency,
        ColumnRole::ImpedanceReal,
        ColumnRole::ImpedanceImaginary,
        ColumnRole::ImpedanceMagnitude,
        ColumnRole::ImpedancePhase,
    ] {
        let matches: Vec<usize> = resolved
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (candidate.as_ref() == Some(&role)).then_some(index))
            .collect();
        if matches.len() > 1 {
            let index = matches[1];
            let diagnostic = context(
                Diagnostic::new(
                    Severity::Error,
                    DiagnosticCode::EisSchema,
                    "EIS role occurs in multiple source columns; no first-column fallback was applied",
                )
                .at(header_index + 1, Some(index + 1))
                .for_column(trim_cell(&header[index]))
                .with_candidate_roles(vec![role.clone()])
                .with_resolution("rejected: duplicate EIS role"),
            );
            return Err(Error::EisSchemaConflict {
                details: Box::new(EisSchemaError {
                    path: path.to_path_buf(),
                    format_candidate,
                    row: header_index + 1,
                    column: index + 1,
                    header: trim_cell(&header[index]).to_owned(),
                    candidate_roles: vec![role.clone()],
                    conflicting_role: Some(role),
                    diagnostics: vec![diagnostic],
                }),
            });
        }
    }

    for role in [
        ColumnRole::Frequency,
        ColumnRole::ImpedanceReal,
        ColumnRole::ImpedanceImaginary,
    ] {
        if !resolved
            .iter()
            .any(|candidate| candidate.as_ref() == Some(&role))
        {
            let detected_roles = resolved.iter().flatten().cloned().collect();
            let diagnostic = context(
                Diagnostic::new(
                    Severity::Error,
                    DiagnosticCode::EisSchema,
                    format!("required EIS role {role:?} is absent from the source header"),
                )
                .at(header_index + 1, None)
                .with_candidate_roles(Vec::new())
                .with_source_headers(header.to_vec())
                .with_resolution("rejected: missing required EIS role"),
            );
            return Err(Error::MissingRequiredRole {
                path: path.to_path_buf(),
                format_candidate,
                role,
                detected_roles,
                diagnostics: vec![diagnostic],
            });
        }
    }
    Ok(())
}

impl FormatHandler for ChiEisHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiEis
    }
    fn detect(&self, probe: &Probe, options: &ReadOptions) -> Option<Detection> {
        let mode = if options.forced_format.as_ref() == Some(&FormatId::ChiEis) {
            EisDispatchMode::Forced
        } else {
            EisDispatchMode::Automatic
        };
        let technique = technique_row(probe, eis_technique).map(|(_, technique)| technique);
        let (header, data) = match mode {
            EisDispatchMode::Automatic => eis_header_and_data(probe, |row| {
                // Preserve automatic confidence requirements: a generic
                // two-column table with one EIS-like header is not EIS.
                let explicit_two_column_header = technique.is_some()
                    && row.len() == 2
                    && row
                        .iter()
                        .any(|value| !eis_header_candidates(value).is_empty());
                eis_schema_candidate(row)
                    || (technique.is_some() && row.len() >= 3)
                    || explicit_two_column_header
            }),
            EisDispatchMode::Forced => forced_eis_header_and_data(probe),
        }?;
        let header_row = probe.row(header)?;
        let width = header_row.len();
        let roles: Vec<_> = header_row
            .iter()
            .map(|value| eis_header_candidates(value))
            .collect();
        let complete = [
            ColumnRole::Frequency,
            ColumnRole::ImpedanceReal,
            ColumnRole::ImpedanceImaginary,
        ]
        .iter()
        .all(|role| {
            roles
                .iter()
                .filter(|candidates| candidates.as_slice() == [role.clone()])
                .count()
                == 1
        });
        Some(Detection {
            format: FormatId::ChiEis,
            confidence: match (technique.as_deref(), complete) {
                (Some(technique), true) => eis_confidence(technique),
                (Some(_), false) => 0.91,
                (None, true) => 0.93,
                (None, false) => 0.89,
            },
            evidence: vec![
                technique
                    .map(|value| format!("EIS technique signature: {value}"))
                    .unwrap_or_else(|| "EIS semantic header without an instrument preamble".into()),
                if complete {
                    "frequency and required complex impedance roles resolved independent of position"
                        .into()
                } else {
                    "EIS-like semantic header requires structured required-role validation".into()
                },
                format!("matched source header: {}", probe.row(header)?.join(", ")),
            ],
            layout: DetectedLayout {
                header_row: Some(header),
                data_start_row: data,
                metadata_range: Some(0..header),
                expected_column_count: width,
                has_header: true,
            },
        })
    }
    fn columns(
        &self,
        probe: &Probe,
        detection: &Detection,
    ) -> crate::Result<Vec<ColumnDescriptor>> {
        let row = probe
            .row(detection.layout.header_row.ok_or(Error::HeaderNotFound)?)
            .ok_or(Error::HeaderNotFound)?;
        row.iter()
            .map(|source| {
                let role = eis_header_candidates(source)
                    .into_iter()
                    .next()
                    .unwrap_or(ColumnRole::Unknown);
                let stem = match role {
                    ColumnRole::Frequency => "frequency",
                    ColumnRole::ImpedanceReal => "z_real",
                    ColumnRole::ImpedanceImaginary => "z_imag",
                    ColumnRole::ImpedanceMagnitude => "z_magnitude",
                    ColumnRole::ImpedancePhase => "phase",
                    ColumnRole::Unknown => "column",
                    _ => return Err(Error::InvalidColumnSignature(source.clone())),
                };
                let unit = header_unit(source).unwrap_or(Unit::Unknown);
                Ok(descriptor(source, &canonical_name(stem, &unit), role, unit))
            })
            .collect()
    }
}

fn ocpt_technique(value: &str) -> Option<&'static str> {
    let compact: String = value
        .chars()
        .filter(|character| !character.is_whitespace() && !matches!(character, '.' | '-'))
        .collect();
    if compact.contains("opencircuitpotentialtime") {
        Some("Open Circuit Potential - Time")
    } else if value.contains("open-circuit potential") {
        Some("Open-Circuit Potential")
    } else if value.contains("open circuit potential") {
        Some("Open Circuit Potential")
    } else if value == "ocpt" || value.contains(" ocpt") {
        Some("OCPT")
    } else if value == "ocp" || value.contains(" ocp") {
        Some("OCP")
    } else {
        None
    }
}

fn eis_technique(value: &str) -> Option<&'static str> {
    let compact: String = value
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '.')
        .collect();
    if compact.contains("acimpedance") {
        Some("A.C. Impedance")
    } else if value.contains("ac impedance") {
        Some("AC Impedance")
    } else if value.contains("electrochemical impedance spectroscopy") {
        Some("Electrochemical Impedance Spectroscopy")
    } else if value == "eis" || value.starts_with("eis ") {
        Some("EIS")
    } else {
        None
    }
}

fn ocpt_confidence(technique: &str) -> f32 {
    match technique {
        "Open Circuit Potential - Time" => 0.99,
        "Open-Circuit Potential" | "Open Circuit Potential" => 0.95,
        "OCPT" => 0.88,
        "OCP" => 0.82,
        _ => 0.80,
    }
}

fn eis_confidence(technique: &str) -> f32 {
    match technique {
        "A.C. Impedance" => 1.0,
        "AC Impedance" | "Electrochemical Impedance Spectroscopy" => 0.97,
        "EIS" => 0.85,
        _ => 0.80,
    }
}

fn descriptor(source: &str, canonical: &str, role: ColumnRole, unit: Unit) -> ColumnDescriptor {
    ColumnDescriptor {
        source_name: trim_cell(source).to_owned(),
        canonical_name: canonical.into(),
        role,
        unit: Some(unit),
    }
}

fn canonical_name(stem: &str, unit: &Unit) -> String {
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
