use super::{DetectedLayout, Detection, FormatHandler, FormatId, trim_cell};
use crate::{ColumnDescriptor, ColumnRole, Error, Probe, ReadOptions, Unit};

pub(crate) struct ChiOcptHandler;
pub(crate) struct ChiMultichannelOcptHandler;
pub(crate) struct ChiEisHandler;

fn normalized(value: &str) -> String {
    trim_cell(value)
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn technique_row(probe: &Probe, expected: &str) -> Option<usize> {
    probe
        .rows
        .iter()
        .position(|row| row.iter().any(|cell| normalized(cell).contains(expected)))
}

fn header_and_data(probe: &Probe, predicate: impl Fn(&[String]) -> bool) -> Option<(usize, usize)> {
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

fn is_time(value: &str) -> bool {
    matches!(
        normalized(value).as_str(),
        "time/s" | "time/sec" | "time(seconds)" | "time"
    )
}

fn is_potential(value: &str) -> bool {
    matches!(
        normalized(value).as_str(),
        "potential/v" | "potential(volt)" | "potential"
    )
}

fn channel_number(value: &str) -> Option<u32> {
    let compact = normalized(value);
    let stem = compact.strip_suffix("/v").unwrap_or(&compact);
    let digits = stem.strip_prefix("ch").or_else(|| stem.strip_prefix('e'))?;
    let parsed = digits.parse::<u32>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn ocpt_header(row: &[String]) -> bool {
    row.len() >= 2 && row.first().is_some_and(|cell| is_time(cell))
}

impl FormatHandler for ChiOcptHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiOcpt
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        technique_row(probe, "opencircuitpotential-time")?;
        let (header, data) = header_and_data(probe, |row| {
            ocpt_header(row) && row.len() == 2 && row.get(1).is_some_and(|v| is_potential(v))
        })?;
        Some(Detection {
            format: FormatId::ChiOcpt,
            confidence: 0.99,
            evidence: vec![
                "Open Circuit Potential - Time technique".into(),
                "time and conventional potential header roles".into(),
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
        Ok(vec![
            descriptor(&row[0], "time_s", ColumnRole::Time, Unit::Second),
            descriptor(&row[1], "potential_v", ColumnRole::Potential, Unit::Volt),
        ])
    }
}

impl FormatHandler for ChiMultichannelOcptHandler {
    fn format_id(&self) -> FormatId {
        FormatId::ChiMultichannelOcpt
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        technique_row(probe, "opencircuitpotential-time")?;
        let (header, data) = header_and_data(probe, |row| {
            row.len() >= 2
                && row.first().is_some_and(|v| is_time(v))
                && row.iter().skip(1).all(|v| channel_number(v).is_some())
        })?;
        let width = probe.row(header)?.len();
        Some(Detection {
            format: FormatId::ChiMultichannelOcpt,
            confidence: 0.995,
            evidence: vec![
                "Open Circuit Potential - Time technique".into(),
                "one or more numbered potential channel headers".into(),
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
            let channel = channel_number(value)
                .ok_or_else(|| Error::InvalidColumnSignature(value.clone()))?;
            result.push(descriptor(
                value,
                &format!("ch{channel}_v"),
                ColumnRole::PotentialChannel(channel),
                Unit::Volt,
            ));
        }
        Ok(result)
    }
}

fn eis_roles(row: &[String]) -> bool {
    if row.len() != 5 {
        return false;
    }
    let values: Vec<String> = row.iter().map(|v| normalized(v)).collect();
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
        technique_row(probe, "a.c.impedance")?;
        let (header, data) = header_and_data(probe, eis_roles)?;
        Some(Detection {
            format: FormatId::ChiEis,
            confidence: 1.0,
            evidence: vec![
                "A.C. Impedance technique".into(),
                "frequency, impedance component, magnitude, and phase roles".into(),
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

fn descriptor(source: &str, canonical: &str, role: ColumnRole, unit: Unit) -> ColumnDescriptor {
    ColumnDescriptor {
        source_name: trim_cell(source).to_owned(),
        canonical_name: canonical.into(),
        role,
        unit: Some(unit),
    }
}
