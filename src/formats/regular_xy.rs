use super::{DetectedLayout, Detection, FormatHandler, FormatId, numeric, trim_cell};
use crate::{ColumnDescriptor, ColumnRole, HeaderPolicy, Probe, ReadOptions, Unit};

pub(crate) struct RegularXyHandler;

impl FormatHandler for RegularXyHandler {
    fn format_id(&self) -> FormatId {
        FormatId::RegularXy
    }

    fn detect(&self, probe: &Probe, options: &ReadOptions) -> Option<Detection> {
        let nonempty: Vec<(usize, &[String])> = probe
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.iter().any(|cell| !trim_cell(cell).is_empty()))
            .map(|(index, row)| (index, row.as_slice()))
            .take(5)
            .collect();
        let (first_index, first) = *nonempty.first()?;
        if first.len() != 2 {
            return None;
        }
        let first_numeric = first.iter().all(|cell| numeric(cell));
        let has_header = match options.header_policy {
            HeaderPolicy::Auto => !first_numeric,
            HeaderPolicy::Present => true,
            HeaderPolicy::Absent => false,
        };
        let data_position = if has_header { 1 } else { 0 };
        let candidate_rows: Vec<&&[String]> = nonempty
            .iter()
            .skip(data_position)
            .take(3)
            .map(|(_, row)| row)
            .collect();
        let shaped_rows = candidate_rows.iter().filter(|row| row.len() == 2).count();
        let numeric_rows = candidate_rows
            .iter()
            .filter(|row| {
                row.len() == 2
                    && row
                        .iter()
                        .all(|cell| trim_cell(cell).is_empty() || numeric(cell))
            })
            .count();
        if shaped_rows < 1 || numeric_rows < 1 {
            return None;
        }
        let data_start_row = nonempty.get(data_position)?.0;
        Some(Detection {
            format: FormatId::RegularXy,
            confidence: if numeric_rows >= 3 {
                0.86
            } else if shaped_rows >= 2 {
                0.78
            } else {
                0.70
            },
            evidence: vec![
                "two consistently shaped numeric columns".into(),
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
                expected_column_count: 2,
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
        Ok(vec![
            ColumnDescriptor {
                source_name: source
                    .and_then(|r| r.first())
                    .map(|v| trim_cell(v).to_owned())
                    .unwrap_or_default(),
                canonical_name: "x".into(),
                role: ColumnRole::X,
                unit: header_unit(source.and_then(|r| r.first())),
            },
            ColumnDescriptor {
                source_name: source
                    .and_then(|r| r.get(1))
                    .map(|v| trim_cell(v).to_owned())
                    .unwrap_or_default(),
                canonical_name: "y".into(),
                role: ColumnRole::Y,
                unit: header_unit(source.and_then(|r| r.get(1))),
            },
        ])
    }
}

fn header_unit(value: Option<&String>) -> Option<Unit> {
    let lower = value?.to_ascii_lowercase();
    if lower.contains("mv") {
        Some(Unit::Other("mV".into()))
    } else if lower.contains("/ s") || lower.contains("/s") {
        Some(Unit::Second)
    } else if lower.contains("/ v") || lower.contains("/v") {
        Some(Unit::Volt)
    } else {
        None
    }
}
