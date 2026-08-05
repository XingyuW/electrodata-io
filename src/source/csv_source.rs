use crate::source::{ContainerFormat, Probe};
use crate::{Diagnostic, DiagnosticCode, Error, Result, Severity};
use std::io::{BufRead, BufReader};
use std::path::Path;

pub(crate) struct CsvInput {
    pub probe: Probe,
    pub rows: Vec<Vec<String>>,
    pub raw_rows: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

pub(crate) fn open(path: &Path, probe_limit: usize, allow_lossy: bool) -> Result<CsvInput> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let (text, diagnostics) = match std::str::from_utf8(&bytes) {
        Ok(value) => (value.to_owned(), Vec::new()),
        Err(_error) if allow_lossy => (
            String::from_utf8_lossy(&bytes).into_owned(),
            vec![Diagnostic {
                severity: Severity::Warning,
                code: DiagnosticCode::LossyDecoding,
                message:
                    "input contained invalid UTF-8 and was decoded with replacement characters"
                        .into(),
                row: None,
                column: None,
            }],
        ),
        Err(error) => {
            return Err(Error::InvalidUtf8 {
                valid_up_to: error.valid_up_to(),
            });
        }
    };
    let mut raw_rows: Vec<String> = text.lines().map(str::to_owned).collect();
    if let Some(first) = raw_rows.first_mut() {
        *first = first.trim_start_matches('\u{feff}').to_owned();
    }
    let delimiter = detect_delimiter(&raw_rows);
    let rows = raw_rows
        .iter()
        .map(|line| parse_line(line, delimiter))
        .collect::<Result<Vec<_>>>()?;
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    let probe = Probe {
        rows: rows.iter().take(probe_limit).cloned().collect(),
        raw_rows: raw_rows.iter().take(probe_limit).cloned().collect(),
        source_name,
        delimiter: Some(delimiter),
        container: ContainerFormat::Csv,
        worksheet_name: None,
    };
    Ok(CsvInput {
        probe,
        rows,
        raw_rows,
        diagnostics,
    })
}

/// Reads only a bounded CSV preview for content detection.
pub(crate) fn probe(path: &Path, probe_limit: usize) -> Result<Probe> {
    let file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut raw_rows = Vec::with_capacity(probe_limit);
    for _ in 0..probe_limit {
        let mut bytes = Vec::new();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        let line = std::str::from_utf8(&bytes).map_err(|error| Error::InvalidUtf8 {
            valid_up_to: error.valid_up_to(),
        })?;
        raw_rows.push(line.to_owned());
    }
    if let Some(first) = raw_rows.first_mut() {
        *first = first.trim_start_matches('\u{feff}').to_owned();
    }
    let delimiter = detect_delimiter(&raw_rows);
    let rows = raw_rows
        .iter()
        .map(|line| parse_line(line, delimiter))
        .collect::<Result<Vec<_>>>()?;
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    Ok(Probe {
        rows,
        raw_rows,
        source_name,
        delimiter: Some(delimiter),
        container: ContainerFormat::Csv,
        worksheet_name: None,
    })
}

fn detect_delimiter(lines: &[String]) -> u8 {
    let samples: Vec<&str> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .take(64)
        .map(String::as_str)
        .collect();
    (*b",\t;")
        .into_iter()
        .map(|delimiter| {
            let counts: Vec<usize> = samples
                .iter()
                .map(|line| {
                    line.as_bytes()
                        .iter()
                        .filter(|byte| **byte == delimiter)
                        .count()
                })
                .collect();
            let rows_with_delimiter = counts.iter().filter(|count| **count > 0).count();
            let total_delimiters = counts.iter().sum::<usize>();
            (delimiter, (rows_with_delimiter, total_delimiters))
        })
        .max_by_key(|(_, score)| *score)
        .filter(|(_, (_, total))| *total > 0)
        .map(|(delimiter, _)| delimiter)
        .unwrap_or(b',')
}

fn parse_line(line: &str, delimiter: u8) -> Result<Vec<String>> {
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(line.as_bytes());
    match reader.records().next() {
        Some(record) => record
            .map(|record| record.iter().map(str::to_owned).collect())
            .map_err(|error| Error::Metadata {
                row: 0,
                message: error.to_string(),
            }),
        None => Ok(Vec::new()),
    }
}
