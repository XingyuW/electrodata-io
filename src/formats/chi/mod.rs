use super::regular_xy::{header_role, header_unit};
use super::{DetectedLayout, Detection, FormatHandler, FormatId, trim_cell};
use crate::detection::normalize_for_match;
use crate::{ColumnDescriptor, ColumnRole, Error, Probe, ReadOptions, Unit};

pub(crate) struct ChiOcptHandler;
pub(crate) struct ChiMultichannelOcptHandler;
pub(crate) struct ChiEisHandler;

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

fn eis_roles(row: &[String]) -> bool {
    if !(3..=5).contains(&row.len()) {
        return false;
    }
    let roles: Vec<ColumnRole> = row.iter().map(|value| header_role(value)).collect();
    let allowed = roles.iter().all(|role| {
        matches!(
            role,
            ColumnRole::Frequency
                | ColumnRole::ImpedanceReal
                | ColumnRole::ImpedanceImaginary
                | ColumnRole::ImpedanceMagnitude
                | ColumnRole::ImpedancePhase
        )
    });
    allowed
        && unique_role(&roles, &ColumnRole::Frequency)
        && roles.iter().any(|role| {
            matches!(
                role,
                ColumnRole::ImpedanceReal
                    | ColumnRole::ImpedanceImaginary
                    | ColumnRole::ImpedanceMagnitude
                    | ColumnRole::ImpedancePhase
            )
        })
        && [
            ColumnRole::ImpedanceReal,
            ColumnRole::ImpedanceImaginary,
            ColumnRole::ImpedanceMagnitude,
            ColumnRole::ImpedancePhase,
        ]
        .iter()
        .all(|expected| roles.iter().filter(|role| *role == expected).count() <= 1)
}

fn unique_role(roles: &[ColumnRole], expected: &ColumnRole) -> bool {
    roles.iter().filter(|role| *role == expected).count() == 1
}

impl FormatHandler for ChiEisHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiEis
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        let technique = technique_row(probe, eis_technique).map(|(_, technique)| technique);
        let (header, data) = header_and_data(probe, eis_roles)?;
        let header_row = probe.row(header)?;
        let width = header_row.len();
        let roles: Vec<_> = header_row.iter().map(|value| header_role(value)).collect();
        let complete = [
            ColumnRole::Frequency,
            ColumnRole::ImpedanceReal,
            ColumnRole::ImpedanceImaginary,
        ]
        .iter()
        .all(|role| unique_role(&roles, role));
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
                let role = header_role(source);
                let stem = match role {
                    ColumnRole::Frequency => "frequency",
                    ColumnRole::ImpedanceReal => "z_real",
                    ColumnRole::ImpedanceImaginary => "z_imag",
                    ColumnRole::ImpedanceMagnitude => "z_magnitude",
                    ColumnRole::ImpedancePhase => "phase",
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
