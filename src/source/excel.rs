use crate::reader::{DataReader, SourceData};
use crate::source::{ContainerFormat, Probe};
use crate::{Error, ReadOptions, Result, SheetSelector};
use calamine::{Data, Reader, open_workbook_auto};
use std::path::Path;

struct SheetData {
    name: String,
    rows: Vec<Vec<String>>,
    raw_rows: Vec<String>,
    probe: Probe,
}

pub(crate) fn read(
    reader: &DataReader,
    path: &Path,
    container: ContainerFormat,
    options: &ReadOptions,
) -> Result<crate::Dataset> {
    let mut workbook = open_workbook_auto(path).map_err(|error| Error::WorkbookOpening {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let names = workbook.sheet_names();
    let selected_names: Vec<String> = match &options.sheet {
        SheetSelector::Auto => names,
        SheetSelector::Name(name) => {
            if !names.iter().any(|candidate| candidate == name) {
                return Err(Error::MissingWorksheet(name.clone()));
            }
            vec![name.clone()]
        }
        SheetSelector::Index(index) => vec![
            names
                .get(*index)
                .cloned()
                .ok_or_else(|| Error::MissingWorksheet(format!("index {index}")))?,
        ],
    };
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut sheets = Vec::new();
    for name in selected_names {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|error| Error::Calamine(error.to_string()))?;
        if range.is_empty() {
            continue;
        }
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|row| row.iter().map(cell_text).collect())
            .collect();
        let raw_rows: Vec<String> = rows.iter().map(|row| row.join(",")).collect();
        let probe = Probe {
            rows: rows.iter().take(options.probe_rows).cloned().collect(),
            raw_rows: raw_rows.iter().take(options.probe_rows).cloned().collect(),
            source_name: source_name.clone(),
            delimiter: None,
            container,
            worksheet_name: Some(name.clone()),
        };
        sheets.push(SheetData {
            name,
            rows,
            raw_rows,
            probe,
        });
    }
    if sheets.is_empty() {
        return Err(Error::EmptyData);
    }
    if !matches!(options.sheet, SheetSelector::Auto) {
        let sheet = sheets.into_iter().next().ok_or(Error::EmptyData)?;
        let (handler, detection) = reader.choose(&sheet.probe, options)?;
        return reader.assemble(
            path,
            SourceData {
                probe: sheet.probe,
                rows: sheet.rows,
                raw_rows: sheet.raw_rows,
                diagnostics: Vec::new(),
            },
            handler,
            detection,
            options,
        );
    }
    let mut candidates = Vec::new();
    for sheet in sheets {
        if let Ok((handler, detection)) = reader.choose(&sheet.probe, options) {
            candidates.push((sheet, handler, detection));
        }
    }
    candidates.sort_by(|left, right| right.2.confidence.total_cmp(&left.2.confidence));
    if candidates.is_empty() {
        return Err(Error::UnknownFormat {
            best_confidence: 0.0,
            threshold: options.detection_threshold,
        });
    }
    if candidates.get(1).is_some_and(|second| {
        (candidates[0].2.confidence - second.2.confidence).abs() < f32::EPSILON
    }) {
        return Err(Error::AmbiguousFormat {
            candidates: candidates
                .iter()
                .take(3)
                .map(|(sheet, _, detection)| {
                    (
                        crate::FormatId::Custom(format!(
                            "worksheet:{}:{}",
                            sheet.name, detection.format
                        )),
                        detection.confidence,
                    )
                })
                .collect(),
        });
    }
    let (sheet, handler, detection) = candidates.remove(0);
    reader.assemble(
        path,
        SourceData {
            probe: sheet.probe,
            rows: sheet.rows,
            raw_rows: sheet.raw_rows,
            diagnostics: Vec::new(),
        },
        handler,
        detection,
        options,
    )
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::Int(value) => value.to_string(),
        Data::Float(value) => value.to_string(),
        Data::String(value) => value.clone(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.to_string(),
        Data::DateTimeIso(value) | Data::DurationIso(value) => value.clone(),
        Data::Error(value) => format!("#{value:?}"),
    }
}
