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
            .map(ColumnRole::PotentialChannel)
            .unwrap_or(ColumnRole::Potential);
        Ok(vec![
            descriptor(&row[0], "time_s", ColumnRole::Time, Unit::Second),
            descriptor(&row[1], "potential_v", potential_role, Unit::Volt),
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
        let mut result = vec![descriptor(
            &row[0],
            "time_s",
            ColumnRole::Time,
            Unit::Second,
        )];
        for value in row.iter().skip(1) {
            if let Some(channel) = channel_number(value) {
                result.push(descriptor(
                    value,
                    &format!("ch{channel}_v"),
                    ColumnRole::PotentialChannel(channel),
                    Unit::Volt,
                ));
            } else if is_potential(value) {
                result.push(descriptor(
                    value,
                    "potential_v",
                    ColumnRole::Potential,
                    Unit::Volt,
                ));
            } else {
                return Err(Error::InvalidColumnSignature(value.clone()));
            }
        }
        Ok(result)
    }
}

fn eis_roles(row: &[String]) -> bool {
    if row.len() != 5 {
        return false;
    }
    let values: Vec<String> = row.iter().map(|v| header_key(v)).collect();
    matches!(values[0].as_str(), "freq/hz" | "frequency/hz")
        && matches!(values[1].as_str(), "z'/ohm" | "zreal/ohm")
        && matches!(values[2].as_str(), "z\"/ohm" | "zimag/ohm")
        && matches!(values[3].as_str(), "z/ohm" | "|z|/ohm")
        && matches!(values[4].as_str(), "phase/deg" | "phase/degree")
}

impl FormatHandler for ChiEisHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiEis
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        let (_, technique) = technique_row(probe, eis_technique)?;
        let (header, data) = header_and_data(probe, eis_roles)?;
        Some(Detection {
            format: FormatId::ChiEis,
            confidence: eis_confidence(&technique),
            evidence: vec![
                format!("EIS technique signature: {technique}"),
                "frequency, impedance component, magnitude, and phase roles".into(),
                format!("matched source header: {}", probe.row(header)?.join(", ")),
            ],
            layout: DetectedLayout {
                header_row: Some(header),
                data_start_row: data,
                metadata_range: Some(0..header),
                expected_column_count: 5,
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
        let definitions = [
            ("frequency_hz", ColumnRole::Frequency, Unit::Hertz),
            ("z_real_ohm", ColumnRole::ImpedanceReal, Unit::Ohm),
            ("z_imag_ohm", ColumnRole::ImpedanceImaginary, Unit::Ohm),
            ("z_magnitude_ohm", ColumnRole::ImpedanceMagnitude, Unit::Ohm),
            ("phase_deg", ColumnRole::Phase, Unit::Degree),
        ];
        Ok(row
            .iter()
            .zip(definitions)
            .map(|(source, (name, role, unit))| descriptor(source, name, role, unit))
            .collect())
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
