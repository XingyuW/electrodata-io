use crate::reader::{DataReader, FormulaCell, SourceData};
use crate::source::{ContainerFormat, Probe};
use crate::{
    Detection, DetectionResult, Error, ReadOptions, Result, SheetSelector, WorksheetCandidate,
};
use calamine::{Data, Reader, open_workbook_auto};
use std::path::Path;

struct SheetData {
    name: String,
    index: usize,
    rows: Vec<Vec<String>>,
    raw_rows: Vec<String>,
    probe: Probe,
    formula_cells: Vec<FormulaCell>,
}

struct CompatibleWorksheet<T> {
    candidate: WorksheetCandidate,
    value: T,
}

fn resolve_worksheet_candidates<T>(
    path: &Path,
    selector: &SheetSelector,
    mut candidates: Vec<CompatibleWorksheet<T>>,
) -> Result<T> {
    match candidates.len() {
        0 => Err(Error::NoCompatibleWorksheet {
            path: path.to_path_buf(),
        }),
        1 => Ok(candidates.remove(0).value),
        _ if matches!(selector, SheetSelector::Auto) => Err(Error::AmbiguousWorksheet {
            path: path.to_path_buf(),
            candidates: candidates
                .into_iter()
                .map(|candidate| candidate.candidate)
                .collect(),
        }),
        _ => Ok(candidates.remove(0).value),
    }
}

fn worksheet_candidate(index: usize, name: String, detection: &Detection) -> WorksheetCandidate {
    WorksheetCandidate {
        name,
        index,
        format: detection.format.clone(),
        confidence: detection.confidence,
        evidence: detection.evidence.clone(),
    }
}

/// Detects a format using the same worksheet selection policy as workbook reads.
pub(crate) fn detect(
    reader: &DataReader,
    path: &Path,
    container: ContainerFormat,
    options: &ReadOptions,
) -> Result<DetectionResult> {
    let mut workbook = open_workbook_auto(path).map_err(|error| Error::WorkbookOpening {
        path: path.to_path_buf(),
        source: Box::new(error),
    })?;
    let names = workbook.sheet_names();
    let selected_sheets: Vec<(usize, String)> = match &options.sheet {
        SheetSelector::Auto => names.into_iter().enumerate().collect(),
        SheetSelector::Name(name) => {
            let index = names
                .iter()
                .position(|candidate| candidate == name)
                .ok_or_else(|| Error::MissingWorksheet {
                    path: path.to_path_buf(),
                    worksheet: name.clone(),
                })?;
            vec![(index, name.clone())]
        }
        SheetSelector::Index(index) => {
            vec![
                names
                    .get(*index)
                    .cloned()
                    .map(|name| (*index, name))
                    .ok_or_else(|| Error::MissingWorksheet {
                        path: path.to_path_buf(),
                        worksheet: format!("index {index}"),
                    })?,
            ]
        }
    };
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut candidates = Vec::new();
    for (index, name) in selected_sheets {
        let range = workbook.worksheet_range(&name).map_err(|error| {
            Error::Calamine {
                path: path.to_path_buf(),
                source: Box::new(error),
            }
            .with_read_context_at(path, Some(&name), Some(index), None)
        })?;
        if range.is_empty() {
            continue;
        }
        let rows: Vec<Vec<String>> = range
            .rows()
            .take(options.probe_rows)
            .map(|row| row.iter().map(cell_text).collect())
            .collect();
        let raw_rows: Vec<String> = rows.iter().map(|row| row.join(",")).collect();
        let probe = Probe {
            rows,
            raw_rows,
            source_name: source_name.clone(),
            delimiter: None,
            container,
            worksheet_name: Some(name.clone()),
            worksheet_index: Some(index),
        };
        if let Ok(mut result) = reader.detect_probe(path, &probe, options) {
            result.worksheet = probe.worksheet_name.clone();
            candidates.push(CompatibleWorksheet {
                candidate: WorksheetCandidate {
                    name: name.clone(),
                    index,
                    format: result.format.into(),
                    confidence: result.confidence,
                    evidence: result.matched_patterns.clone(),
                },
                value: result,
            });
        }
    }
    resolve_worksheet_candidates(path, &options.sheet, candidates)
}

pub(crate) fn read(
    reader: &DataReader,
    path: &Path,
    container: ContainerFormat,
    options: &ReadOptions,
) -> Result<crate::Dataset> {
    let source_bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let source_sha256 = crate::source::sha256_hex(&source_bytes);
    let mut workbook = open_workbook_auto(path).map_err(|error| Error::WorkbookOpening {
        path: path.to_path_buf(),
        source: Box::new(error),
    })?;
    let names = workbook.sheet_names();
    let selected_sheets: Vec<(usize, String)> = match &options.sheet {
        SheetSelector::Auto => names.into_iter().enumerate().collect(),
        SheetSelector::Name(name) => {
            let index = names
                .iter()
                .position(|candidate| candidate == name)
                .ok_or_else(|| Error::MissingWorksheet {
                    path: path.to_path_buf(),
                    worksheet: name.clone(),
                })?;
            vec![(index, name.clone())]
        }
        SheetSelector::Index(index) => {
            vec![
                names
                    .get(*index)
                    .cloned()
                    .map(|name| (*index, name))
                    .ok_or_else(|| Error::MissingWorksheet {
                        path: path.to_path_buf(),
                        worksheet: format!("index {index}"),
                    })?,
            ]
        }
    };
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut sheets = Vec::new();
    let mut explicitly_selected_empty_sheet = None;
    for (index, name) in selected_sheets {
        let range = workbook.worksheet_range(&name).map_err(|error| {
            Error::Calamine {
                path: path.to_path_buf(),
                source: Box::new(error),
            }
            .with_read_context_at(path, Some(&name), Some(index), None)
        })?;
        let formulas = workbook.worksheet_formula(&name).map_err(|error| {
            Error::Calamine {
                path: path.to_path_buf(),
                source: Box::new(error),
            }
            .with_read_context_at(path, Some(&name), Some(index), None)
        })?;
        if range.is_empty() {
            if !matches!(options.sheet, SheetSelector::Auto) {
                explicitly_selected_empty_sheet = Some((name, index));
            }
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
            worksheet_index: Some(index),
        };
        sheets.push(SheetData {
            name,
            index,
            rows,
            raw_rows,
            probe,
            formula_cells: formula_cells(&range, &formulas),
        });
    }
    if sheets.is_empty() {
        if matches!(options.sheet, SheetSelector::Auto) {
            return Err(Error::NoCompatibleWorksheet {
                path: path.to_path_buf(),
            });
        }
        if let Some((name, index)) = explicitly_selected_empty_sheet {
            return Err(Error::EmptyData.with_read_context_at(
                path,
                Some(&name),
                Some(index),
                None,
            ));
        }
        return Err(Error::EmptyData.with_read_context(path, None, None));
    }
    let mut candidates = Vec::new();
    for sheet in sheets {
        match reader.choose(path, &sheet.probe, options) {
            Ok((handler, detection)) => candidates.push(CompatibleWorksheet {
                candidate: worksheet_candidate(sheet.index, sheet.name.clone(), &detection),
                value: (sheet, handler, detection),
            }),
            Err(_) if matches!(options.sheet, SheetSelector::Auto) => continue,
            Err(error) => {
                return Err(error.with_read_context_at(
                    path,
                    Some(&sheet.name),
                    Some(sheet.index),
                    None,
                ));
            }
        }
    }
    let (sheet, handler, detection) =
        resolve_worksheet_candidates(path, &options.sheet, candidates)?;
    let resolved_sheet_name = sheet.name.clone();
    let resolved_sheet_index = sheet.index;
    let resolved_format = detection.format.clone();
    reader
        .assemble(
            path,
            SourceData {
                probe: sheet.probe,
                rows: sheet.rows,
                raw_rows: sheet.raw_rows,
                diagnostics: Vec::new(),
                encoding: None,
                source_sha256,
                formula_cells: sheet.formula_cells,
            },
            handler,
            detection,
            options,
        )
        .map_err(|error| {
            error.with_read_context_at(
                path,
                Some(&resolved_sheet_name),
                Some(resolved_sheet_index),
                Some(resolved_format),
            )
        })
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

fn formula_cells(
    values: &calamine::Range<Data>,
    formulas: &calamine::Range<String>,
) -> Vec<FormulaCell> {
    let Some((start_row, start_column)) = formulas.start() else {
        return Vec::new();
    };
    formulas
        .used_cells()
        .filter(|(_, _, formula)| !formula.is_empty())
        .map(|(row, column, formula)| {
            let absolute = (start_row + row as u32, start_column + column as u32);
            let usable_numeric_cache = matches!(
                values.get_value(absolute),
                Some(Data::Int(_) | Data::Float(_))
            );
            FormulaCell {
                row: absolute.0 as usize + 1,
                column: absolute.1 as usize + 1,
                formula: formula.clone(),
                usable_numeric_cache,
            }
        })
        .collect()
}
