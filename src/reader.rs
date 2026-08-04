//! Registry-based reader orchestration.

use crate::formats::{
    ChiEisHandler, ChiMultichannelOcptHandler, ChiOcptHandler, Detection, FormatHandler,
    RegularXyHandler,
};
use crate::metadata::{
    AcquisitionMetadata, Metadata, MetadataValue, Parameter, Provenance, RawMetadataRow,
};
#[cfg(feature = "csv")]
use crate::source::csv_source;
use crate::source::{ContainerFormat, Probe};
use crate::validation;
use crate::{ColumnNamePolicy, Dataset, Error, ReadOptions, Result};
use chrono::NaiveDateTime;
use polars::prelude::{Column, DataFrame};
use std::path::Path;
use std::sync::Arc;

/// Configured immutable registry and reader.
#[derive(Clone)]
pub struct DataReader {
    handlers: Vec<Arc<dyn FormatHandler>>,
}

/// Builder for [`DataReader`].
#[derive(Default)]
pub struct DataReaderBuilder {
    handlers: Vec<Arc<dyn FormatHandler>>,
}

pub(crate) struct SourceData {
    pub probe: Probe,
    pub rows: Vec<Vec<String>>,
    pub raw_rows: Vec<String>,
    pub diagnostics: Vec<crate::Diagnostic>,
}

impl DataReader {
    /// Starts an empty reader builder.
    pub fn builder() -> DataReaderBuilder {
        DataReaderBuilder::default()
    }

    /// Reads a path with default options.
    pub fn read(&self, path: impl AsRef<Path>) -> Result<Dataset> {
        self.read_with_options(path, &ReadOptions::default())
    }

    /// Reads a path with explicit options.
    pub fn read_with_options(
        &self,
        path: impl AsRef<Path>,
        options: &ReadOptions,
    ) -> Result<Dataset> {
        let path = path.as_ref();
        match container_for(path)? {
            ContainerFormat::Csv => self.read_csv(path, options),
            #[cfg(feature = "excel")]
            container => crate::source::excel::read(self, path, container, options),
            #[cfg(not(feature = "excel"))]
            container => Err(Error::UnsupportedContainer(format!(
                "{container:?}; enable the excel feature"
            ))),
        }
    }

    pub(crate) fn choose<'a>(
        &'a self,
        probe: &Probe,
        options: &ReadOptions,
    ) -> Result<(&'a dyn FormatHandler, Detection)> {
        let mut candidates: Vec<(&dyn FormatHandler, Detection)> = self
            .handlers
            .iter()
            .filter_map(|handler| {
                handler
                    .detect(probe, options)
                    .map(|detection| (handler.as_ref(), detection))
            })
            .collect();
        candidates.sort_by(|left, right| right.1.confidence.total_cmp(&left.1.confidence));
        if let Some(forced) = &options.forced_format {
            return candidates
                .into_iter()
                .find(|(handler, _)| &handler.format_id() == forced)
                .ok_or_else(|| {
                    if self
                        .handlers
                        .iter()
                        .any(|handler| &handler.format_id() == forced)
                    {
                        Error::InvalidColumnSignature(format!(
                            "input is incompatible with forced format {forced}"
                        ))
                    } else {
                        Error::UnregisteredFormat(forced.clone())
                    }
                });
        }
        let best_confidence = candidates
            .first()
            .map(|candidate| candidate.1.confidence)
            .unwrap_or(0.0);
        if best_confidence < options.detection_threshold {
            return Err(Error::UnknownFormat {
                best_confidence,
                threshold: options.detection_threshold,
            });
        }
        if candidates
            .get(1)
            .is_some_and(|second| best_confidence - second.1.confidence < options.detection_margin)
        {
            return Err(Error::AmbiguousFormat {
                candidates: candidates
                    .iter()
                    .take(3)
                    .map(|(_, item)| (item.format.clone(), item.confidence))
                    .collect(),
            });
        }
        candidates.into_iter().next().ok_or(Error::UnknownFormat {
            best_confidence: 0.0,
            threshold: options.detection_threshold,
        })
    }

    fn read_csv(&self, path: &Path, options: &ReadOptions) -> Result<Dataset> {
        #[cfg(not(feature = "csv"))]
        {
            let _ = (path, options);
            return Err(Error::UnsupportedContainer(
                "CSV; enable the csv feature".into(),
            ));
        }
        #[cfg(feature = "csv")]
        {
            let input = csv_source::open(path, options.probe_rows, options.allow_lossy_utf8)?;
            let (handler, detection) = self.choose(&input.probe, options)?;
            self.assemble(
                path,
                SourceData {
                    probe: input.probe,
                    rows: input.rows,
                    raw_rows: input.raw_rows,
                    diagnostics: input.diagnostics,
                },
                handler,
                detection,
                options,
            )
        }
    }

    pub(crate) fn assemble(
        &self,
        path: &Path,
        source: SourceData,
        handler: &dyn FormatHandler,
        detection: Detection,
        options: &ReadOptions,
    ) -> Result<Dataset> {
        let descriptors = handler.columns(&source.probe, &detection)?;
        let data = numeric_frame(
            &source.rows,
            &descriptors,
            &detection,
            options.column_name_policy,
        )?;
        let metadata = parse_metadata(
            path,
            &source.probe,
            &source.rows,
            &source.raw_rows,
            &detection,
        );
        let mut diagnostics = source.diagnostics;
        let mut dataset = Dataset {
            data,
            metadata,
            format: detection.format,
            columns: descriptors,
            diagnostics: Vec::new(),
        };
        validation::validate(&dataset, options, &mut diagnostics)?;
        dataset.diagnostics = diagnostics;
        Ok(dataset)
    }
}

impl DataReaderBuilder {
    /// Registers all built-in scientific handlers.
    pub fn with_builtin_formats(mut self) -> Self {
        self.handlers.extend([
            Arc::new(ChiEisHandler) as Arc<dyn FormatHandler>,
            Arc::new(ChiMultichannelOcptHandler),
            Arc::new(ChiOcptHandler),
            Arc::new(RegularXyHandler),
        ]);
        self
    }

    /// Registers or replaces a scientific format handler.
    pub fn register(mut self, handler: impl FormatHandler + 'static) -> Self {
        let id = handler.format_id();
        self.handlers.retain(|existing| existing.format_id() != id);
        self.handlers.push(Arc::new(handler));
        self
    }

    /// Builds the immutable reader.
    pub fn build(self) -> DataReader {
        DataReader {
            handlers: self.handlers,
        }
    }
}

fn container_for(path: &Path) -> Result<ContainerFormat> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv" | "txt" | "tsv") => Ok(ContainerFormat::Csv),
        Some("xlsx") => Ok(ContainerFormat::Xlsx),
        Some("xls") => Ok(ContainerFormat::Xls),
        Some("xlsb") => Ok(ContainerFormat::Xlsb),
        Some("ods") => Ok(ContainerFormat::Ods),
        Some(value) => Err(Error::UnsupportedContainer(value.into())),
        None => Err(Error::UnsupportedContainer("missing extension".into())),
    }
}

fn numeric_frame(
    rows: &[Vec<String>],
    columns: &[crate::ColumnDescriptor],
    detection: &Detection,
    naming: ColumnNamePolicy,
) -> Result<DataFrame> {
    let expected = detection.layout.expected_column_count;
    let mut values: Vec<Vec<Option<f64>>> = (0..expected).map(|_| Vec::new()).collect();
    for (index, row) in rows
        .iter()
        .enumerate()
        .skip(detection.layout.data_start_row)
    {
        if row.is_empty() || row.iter().all(|cell| cell.trim().is_empty()) {
            if rows[index + 1..]
                .iter()
                .all(|later| later.is_empty() || later.iter().all(|cell| cell.trim().is_empty()))
            {
                break;
            }
            return Err(Error::RaggedRow {
                row: index + 1,
                expected,
                actual: 0,
            });
        }
        if row.len() != expected {
            return Err(Error::RaggedRow {
                row: index + 1,
                expected,
                actual: row.len(),
            });
        }
        for (column, cell) in row.iter().enumerate() {
            let value = cell.trim();
            if value.is_empty() {
                values[column].push(None);
            } else {
                values[column].push(Some(value.parse::<f64>().map_err(|_| {
                    Error::MalformedNumeric {
                        row: index + 1,
                        column: column + 1,
                        value: value.into(),
                    }
                })?));
            }
        }
    }
    if values.first().is_none_or(Vec::is_empty) {
        return Err(Error::EmptyData);
    }
    let polars_columns: Vec<Column> = columns
        .iter()
        .zip(values)
        .map(|(descriptor, values)| {
            let name = match naming {
                ColumnNamePolicy::Canonical => &descriptor.canonical_name,
                ColumnNamePolicy::PreserveSource if !descriptor.source_name.is_empty() => {
                    &descriptor.source_name
                }
                ColumnNamePolicy::PreserveSource => &descriptor.canonical_name,
            };
            Column::new(name.clone().into(), values)
        })
        .collect();
    Ok(DataFrame::new_infer_height(polars_columns)?)
}

fn parse_metadata(
    path: &Path,
    probe: &Probe,
    rows: &[Vec<String>],
    raw_rows: &[String],
    detection: &Detection,
) -> Metadata {
    let range = detection.layout.metadata_range.clone().unwrap_or(0..0);
    let mut acquisition = AcquisitionMetadata::default();
    let mut parameters = Vec::new();
    let mut retained = Vec::new();
    for index in range {
        let cells = rows.get(index).cloned().unwrap_or_default();
        let raw = raw_rows
            .get(index)
            .cloned()
            .unwrap_or_else(|| cells.join(","));
        retained.push(RawMetadataRow {
            row_number: index + 1,
            cells,
            reconstructed_text: raw.clone(),
        });
        let text = raw.trim().trim_start_matches('\u{feff}');
        if text.is_empty() {
            continue;
        }
        if acquisition.recorded_at.is_none() {
            acquisition.recorded_at = parse_chi_datetime(text);
            if acquisition.recorded_at.is_some() {
                continue;
            }
        }
        if text.eq_ignore_ascii_case("Open Circuit Potential - Time")
            || text.eq_ignore_ascii_case("A.C. Impedance")
        {
            acquisition.technique = Some(text.into());
            continue;
        }
        if let Some((key, value)) = text.split_once(':') {
            let value = value.trim().to_owned();
            match key.trim().to_ascii_lowercase().as_str() {
                "file" => acquisition.source_file = Some(value),
                "data source" => acquisition.data_source = Some(value),
                "instrument model" => acquisition.instrument_model = Some(value),
                "header" => acquisition.header = Some(value),
                "note" => acquisition.note = Some(value),
                _ => parameters.push(parameter(text, "", index + 1)),
            }
        } else if let Some((name, value)) = text.split_once('=') {
            parameters.push(parameter(name.trim(), value.trim(), index + 1));
        } else {
            parameters.push(parameter(text, "", index + 1));
        }
    }
    Metadata {
        acquisition,
        parameters,
        raw_rows: retained,
        provenance: Provenance {
            path: path.display().to_string(),
            source_name: probe.source_name.clone(),
            container: probe.container,
            worksheet: probe.worksheet_name.clone(),
        },
    }
}

fn parameter(raw_name: &str, raw_value: &str, row_number: usize) -> Parameter {
    let (name, unit) = raw_name
        .rsplit_once('(')
        .and_then(|(name, tail)| {
            tail.strip_suffix(')')
                .map(|unit| (name.trim(), Some(unit.to_owned())))
        })
        .unwrap_or((raw_name, None));
    let value = if let Ok(integer) = raw_value.parse::<i64>() {
        MetadataValue::Integer(integer)
    } else if let Ok(float) = raw_value.parse::<f64>() {
        MetadataValue::Float(float)
    } else {
        MetadataValue::Text(raw_value.into())
    };
    Parameter {
        name: name.into(),
        raw_name: raw_name.into(),
        unit,
        value,
        raw_value: raw_value.into(),
        row_number,
    }
}

fn parse_chi_datetime(value: &str) -> Option<NaiveDateTime> {
    [
        "%b. %-d, %Y %H:%M:%S",
        "%b %-d, %Y %H:%M:%S",
        "%b. %d, %Y %H:%M:%S",
        "%b %d, %Y %H:%M:%S",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
}
