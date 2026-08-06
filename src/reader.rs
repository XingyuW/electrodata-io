//! Registry-based reader orchestration.

use crate::formats::{
    ChiEisHandler, ChiMultichannelOcptHandler, ChiOcptHandler, Detection, FirstRowClassification,
    FormatHandler, RegularXyHandler, classify_first_row, explicit_comment_row,
};
use crate::metadata::{
    AcquisitionMetadata, FormulaCacheStatus, FormulaProvenance, Metadata, MetadataValue, Parameter,
    Provenance, RawMetadataRow, SourceColumnProvenance,
};
#[cfg(feature = "csv")]
use crate::source::csv_source;
use crate::source::{ContainerFormat, Probe};
use crate::validation;
use crate::{
    ColumnNamePolicy, ColumnRole, CoordinateOrderPolicy, Dataset, DetectionResult, Diagnostic,
    DiagnosticCode, Error, InvalidCellPolicy, InvalidTimePolicy, RaggedRowPolicy, ReadOptions,
    RecoveryAction, Result, Severity, TextEncoding, Unit,
};
use chrono::NaiveDateTime;
use polars::prelude::{Column, DataFrame};
use std::io::Read;
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
    pub encoding: Option<TextEncoding>,
    pub source_sha256: String,
    pub formula_cells: Vec<FormulaCell>,
}

#[derive(Debug, Clone)]
pub(crate) struct FormulaCell {
    pub row: usize,
    pub column: usize,
    pub formula: String,
    pub usable_numeric_cache: bool,
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
            container => Err(Error::UnsupportedContainer {
                path: path.to_path_buf(),
                container: format!("{container:?}; enable the excel feature"),
            }),
        }
    }

    /// Detects a built-in scientific format from a bounded source preview.
    pub fn detect_format(&self, path: impl AsRef<Path>) -> Result<DetectionResult> {
        let path = path.as_ref();
        let options = ReadOptions {
            probe_rows: 160,
            ..ReadOptions::default()
        };
        match container_for(path)? {
            ContainerFormat::Csv => {
                #[cfg(feature = "csv")]
                {
                    let probe = csv_source::probe(path, options.probe_rows)?;
                    self.detect_probe(path, &probe, &options)
                }
                #[cfg(not(feature = "csv"))]
                {
                    let _ = options;
                    Err(Error::UnsupportedContainer {
                        path: path.to_path_buf(),
                        container: "CSV; enable the csv feature".into(),
                    })
                }
            }
            #[cfg(feature = "excel")]
            container => crate::source::excel::detect(self, path, container, &options),
            #[cfg(not(feature = "excel"))]
            container => Err(Error::UnsupportedContainer {
                path: path.to_path_buf(),
                container: format!("{container:?}; enable the excel feature"),
            }),
        }
    }

    pub(crate) fn detect_probe(
        &self,
        path: &Path,
        probe: &Probe,
        options: &ReadOptions,
    ) -> Result<DetectionResult> {
        crate::detection::select(path, self.detections(probe, options), options)
    }

    pub(crate) fn detections(&self, probe: &Probe, options: &ReadOptions) -> Vec<Detection> {
        self.handlers
            .iter()
            .filter_map(|handler| handler.detect(probe, options))
            .collect()
    }

    pub(crate) fn choose<'a>(
        &'a self,
        path: &Path,
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
                        .with_read_context(
                            path,
                            probe.worksheet_name.as_deref(),
                            Some(forced.clone()),
                        )
                    } else {
                        Error::UnregisteredFormat(forced.clone()).with_read_context(
                            path,
                            probe.worksheet_name.as_deref(),
                            Some(forced.clone()),
                        )
                    }
                });
        }
        let best_confidence = candidates
            .first()
            .map(|candidate| candidate.1.confidence)
            .unwrap_or(0.0);
        if best_confidence < options.detection_threshold {
            return Err(Error::UnknownFormat {
                path: path.to_path_buf(),
                best_confidence,
                threshold: options.detection_threshold,
            });
        }
        if candidates
            .get(1)
            .is_some_and(|second| best_confidence - second.1.confidence < options.detection_margin)
        {
            return Err(Error::AmbiguousFormat {
                path: path.to_path_buf(),
                candidates: candidates
                    .iter()
                    .take(3)
                    .map(|(_, item)| (item.format.clone(), item.confidence))
                    .collect(),
            });
        }
        candidates.into_iter().next().ok_or(Error::UnknownFormat {
            path: path.to_path_buf(),
            best_confidence: 0.0,
            threshold: options.detection_threshold,
        })
    }

    fn read_csv(&self, path: &Path, options: &ReadOptions) -> Result<Dataset> {
        #[cfg(not(feature = "csv"))]
        {
            let _ = (path, options);
            Err(Error::UnsupportedContainer {
                path: path.to_path_buf(),
                container: "CSV; enable the csv feature".into(),
            })
        }
        #[cfg(feature = "csv")]
        {
            let input = csv_source::open(path, options.probe_rows, options.allow_lossy_utf8)?;
            let (handler, detection) = self.choose(path, &input.probe, options)?;
            self.assemble(
                path,
                SourceData {
                    probe: input.probe,
                    rows: input.rows,
                    raw_rows: input.raw_rows,
                    diagnostics: input.diagnostics,
                    encoding: Some(input.encoding),
                    source_sha256: input.source_sha256,
                    formula_cells: Vec::new(),
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
        let SourceData {
            probe,
            rows,
            raw_rows,
            diagnostics: source_diagnostics,
            encoding,
            source_sha256,
            formula_cells,
        } = source;
        if detection.format == crate::FormatId::ChiEis {
            crate::formats::validate_eis_schema(path, &probe, &detection, options)?;
        }
        let descriptors = handler.columns(&probe, &detection).map_err(|error| {
            error.with_read_context(
                path,
                probe.worksheet_name.as_deref(),
                Some(detection.format.clone()),
            )
        })?;
        let mut diagnostics: Vec<Diagnostic> = source_diagnostics
            .into_iter()
            .map(|diagnostic| {
                diagnostic.with_context(
                    path,
                    probe.worksheet_name.as_deref(),
                    &detection.format,
                    options.effective_profile(),
                )
            })
            .collect();
        diagnostics.append(&mut regular_row_classification_diagnostics(
            path, &probe, &rows, &detection, options,
        )?);
        if !detection.layout.has_header && !options.permits_headerless_inference() {
            return Err(Error::AmbiguousHeaderlessSchema {
                path: path.to_path_buf(),
                format_candidate: Some(detection.format.clone()),
                diagnostics,
            });
        }
        let mut decoded = numeric_frame(
            &rows,
            DecodeContext {
                path,
                columns: &descriptors,
                detection: &detection,
                naming: options.column_name_policy,
                options,
                worksheet: probe.worksheet_name.as_deref(),
                formula_cells: &formula_cells,
            },
        )
        .map_err(|error| {
            let mut accrued = diagnostics.clone();
            accrued.extend_from_slice(error.diagnostics());
            error.with_read_context_and_diagnostics(
                path,
                probe.worksheet_name.as_deref(),
                Some(detection.format.clone()),
                accrued,
            )
        })?;
        let metadata = parse_metadata(
            &rows,
            &raw_rows,
            MetadataContext {
                path,
                probe: &probe,
                detection: &detection,
                options,
                descriptors: &descriptors,
                formula_cells: &formula_cells,
                encoding,
                source_sha256,
            },
        );
        diagnostics.append(&mut decoded.diagnostics);
        if !detection.layout.has_header {
            diagnostics.push(
                diagnostic(
                    path,
                    probe.worksheet_name.as_deref(),
                    &detection.format,
                    options,
                    Severity::Warning,
                    DiagnosticCode::HeaderlessRoleInference,
                    format!(
                        "first table row classified as numeric/missing data by {:?}; headerless schema inferred with first column Time and remaining columns Unknown",
                        options.header_policy
                    ),
                )
                .at(detection.layout.data_start_row + 1, None)
                .recovered(RecoveryAction::RolesInferred),
            );
            diagnostics.push(
                diagnostic(
                    path,
                    probe.worksheet_name.as_deref(),
                    &detection.format,
                    options,
                    Severity::Warning,
                    DiagnosticCode::UnknownUnit,
                    "headerless columns have unknown source units",
                )
                .at(detection.layout.data_start_row + 1, None)
                .recovered(RecoveryAction::Preserved),
            );
        }
        let mut dataset = Dataset::from_parts(
            decoded.data,
            metadata,
            detection.format.clone(),
            descriptors,
            decoded.values,
            decoded.source_rows,
            decoded.source_data_row_count,
        );
        validate_coordinate_order(path, &dataset, options, &mut diagnostics).map_err(|error| {
            error.with_read_context_and_diagnostics(
                path,
                probe.worksheet_name.as_deref(),
                Some(detection.format.clone()),
                diagnostics.clone(),
            )
        })?;
        validation::validate(&dataset, options, &mut diagnostics).map_err(|error| {
            error.with_read_context_and_diagnostics(
                path,
                probe.worksheet_name.as_deref(),
                Some(detection.format.clone()),
                diagnostics.clone(),
            )
        })?;
        dataset.diagnostics = diagnostics;
        Ok(dataset)
    }
}

fn regular_row_classification_diagnostics(
    path: &Path,
    probe: &Probe,
    rows: &[Vec<String>],
    detection: &Detection,
    options: &ReadOptions,
) -> Result<Vec<Diagnostic>> {
    if detection.format != crate::FormatId::RegularXy {
        return Ok(Vec::new());
    }
    let table_row = detection
        .layout
        .header_row
        .unwrap_or(detection.layout.data_start_row);
    let mut diagnostics = Vec::new();
    for (index, row) in rows.iter().enumerate().take(table_row) {
        let source_row = index + 1;
        if row.is_empty() || row.iter().all(|cell| cell.trim().is_empty()) {
            diagnostics.push(
                diagnostic(
                    path,
                    probe.worksheet_name.as_deref(),
                    &detection.format,
                    options,
                    Severity::Info,
                    DiagnosticCode::RowClassification,
                    format!(
                        "source row classified as empty before the table by {:?}; row ignored",
                        options.header_policy
                    ),
                )
                .at(source_row, None)
                .recovered(RecoveryAction::EmptyRowIgnored),
            );
        } else if explicit_comment_row(row) {
            diagnostics.push(
                diagnostic(
                    path,
                    probe.worksheet_name.as_deref(),
                    &detection.format,
                    options,
                    Severity::Info,
                    DiagnosticCode::RowClassification,
                    format!(
                        "source row classified by explicit # or // comment syntax under {:?}; row preserved as metadata",
                        options.header_policy
                    ),
                )
                .at(source_row, None)
                .recovered(RecoveryAction::MetadataPreserved),
            );
        }
    }

    let row = probe
        .row(table_row)
        .or_else(|| rows.get(table_row).map(Vec::as_slice))
        .ok_or(Error::EmptyData)?;
    let content_classification = classify_first_row(row);
    if options.header_policy == crate::HeaderPolicy::Auto
        && matches!(content_classification, FirstRowClassification::Ambiguous(_))
        && options.effective_profile() == crate::ReadProfile::Strict
    {
        let FirstRowClassification::Ambiguous(reason) = content_classification else {
            unreachable!();
        };
        diagnostics.push(
            diagnostic(
                path,
                probe.worksheet_name.as_deref(),
                &detection.format,
                options,
                Severity::Error,
                DiagnosticCode::RowClassification,
                format!("first table row is ambiguous: {reason}; no recovery was applied"),
            )
            .at(table_row + 1, None),
        );
        return Err(Error::AmbiguousFirstRow {
            path: path.to_path_buf(),
            format_candidate: Some(detection.format.clone()),
            row: table_row + 1,
            reason: reason.into(),
            policy: options.header_policy,
            diagnostics,
        });
    }

    if detection.layout.has_header {
        let (reason, severity) = match content_classification {
            FirstRowClassification::Header(reason) => (reason, Severity::Info),
            FirstRowClassification::Data(reason) => (reason, Severity::Warning),
            FirstRowClassification::Ambiguous(reason) => (reason, Severity::Warning),
        };
        diagnostics.push(
            diagnostic(
                path,
                probe.worksheet_name.as_deref(),
                &detection.format,
                options,
                severity,
                DiagnosticCode::RowClassification,
                format!(
                    "source row interpreted as the table header by {:?}: {reason}",
                    options.header_policy
                ),
            )
            .at(table_row + 1, None)
            .recovered(RecoveryAction::HeaderInterpreted),
        );
    }
    Ok(diagnostics)
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
    let mut file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut prefix = [0_u8; 8192];
    let length = file.read(&mut prefix).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let bytes = &prefix[..length];
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if bytes.starts_with(b"PK\x03\x04") {
        return Ok(match extension.as_str() {
            "ods" => ContainerFormat::Ods,
            "xlsb" => ContainerFormat::Xlsb,
            _ => ContainerFormat::Xlsx,
        });
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Err(Error::UnsupportedContainer {
            path: path.to_path_buf(),
            container: "legacy XLS/OLE workbooks are intentionally unsupported".into(),
        });
    }
    if looks_binary(bytes) {
        let magic = bytes
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join("");
        return Err(Error::UnsupportedBinary {
            path: path.to_path_buf(),
            magic,
            format_candidate: None,
            diagnostics: Vec::new(),
        });
    }
    // Text content wins over the extension. This intentionally includes .dat
    // and permits controlled extensionless/custom-extension imports.
    Ok(ContainerFormat::Csv)
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let suspicious = bytes
        .iter()
        .filter(|byte| **byte == 0 || (**byte < 0x09) || (0x0E..0x20).contains(*byte))
        .count();
    bytes.contains(&0) || suspicious * 20 > bytes.len()
}

struct DecodedTable {
    data: DataFrame,
    values: Vec<Vec<Option<f64>>>,
    source_rows: Vec<usize>,
    source_data_row_count: usize,
    diagnostics: Vec<Diagnostic>,
}

struct DecodeContext<'a> {
    path: &'a Path,
    columns: &'a [crate::ColumnDescriptor],
    detection: &'a Detection,
    naming: ColumnNamePolicy,
    options: &'a ReadOptions,
    worksheet: Option<&'a str>,
    formula_cells: &'a [FormulaCell],
}

fn numeric_frame(rows: &[Vec<String>], context: DecodeContext<'_>) -> Result<DecodedTable> {
    let DecodeContext {
        path,
        columns,
        detection,
        naming,
        options,
        worksheet,
        formula_cells,
    } = context;
    let expected = detection.layout.expected_column_count;
    let mut values: Vec<Vec<Option<f64>>> = (0..expected).map(|_| Vec::new()).collect();
    let mut source_rows = Vec::new();
    let mut diagnostics = Vec::new();
    let mut source_data_row_count = 0;
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
            continue;
        }
        source_data_row_count += 1;
        let source_row = index + 1;
        let mut cells = row.clone();
        let source_cell_count = cells.len();
        if cells.len() != expected {
            if options.ragged_row_policy == RaggedRowPolicy::Error {
                return Err(Error::RaggedRow {
                    path: path.to_path_buf(),
                    format_candidate: Some(detection.format.clone()),
                    row: source_row,
                    expected,
                    actual: cells.len(),
                    diagnostics,
                });
            }
            diagnostics.push(
                diagnostic(
                    path,
                    worksheet,
                    &detection.format,
                    options,
                    Severity::Warning,
                    DiagnosticCode::RaggedRow,
                    format!("source row has {} cells; expected {expected}", cells.len()),
                )
                .at(source_row, None),
            );
            if cells.len() < expected {
                let first_missing = cells.len();
                cells.resize(expected, String::new());
                diagnostics.push(
                    diagnostic(
                        path,
                        worksheet,
                        &detection.format,
                        options,
                        Severity::Warning,
                        DiagnosticCode::RowPadded,
                        format!(
                            "short row padded with {} null cell(s)",
                            expected - first_missing
                        ),
                    )
                    .at(source_row, Some(first_missing + 1))
                    .for_column(column_label(columns, first_missing))
                    .recovered(RecoveryAction::RowPadded),
                );
            } else {
                let extra = cells.len() - expected;
                cells.truncate(expected);
                diagnostics.push(
                    diagnostic(
                        path,
                        worksheet,
                        &detection.format,
                        options,
                        Severity::Warning,
                        DiagnosticCode::ExtraCellsIgnored,
                        format!("ignored {extra} trailing cell(s)"),
                    )
                    .at(source_row, Some(expected + 1))
                    .for_column(format!("column_{}", expected + 1))
                    .recovered(RecoveryAction::ExtraCellsIgnored),
                );
            }
        }
        let mut decoded_row = Vec::with_capacity(expected);
        let mut skip_row = false;
        for (column, cell) in cells.iter().enumerate() {
            let value = cell.trim();
            let coordinate = columns
                .get(column)
                .is_some_and(|descriptor| is_coordinate(&descriptor.role));
            if let Some(formula) = formula_cells.iter().find(|formula| {
                formula.row == source_row
                    && formula.column == column + 1
                    && !formula.usable_numeric_cache
            }) {
                if coordinate && options.invalid_time_policy == InvalidTimePolicy::SkipRow {
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::UnusableFormula,
                            "formula has no usable cached numeric coordinate; source row skipped",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .with_token(&formula.formula)
                        .recovered(RecoveryAction::RowSkipped),
                    );
                    decoded_row.push(None);
                    skip_row = true;
                    continue;
                }
                if !coordinate && options.invalid_cell_policy == InvalidCellPolicy::Null {
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::UnusableFormula,
                            "formula has no usable cached numeric result; value replaced with null",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .with_token(&formula.formula)
                        .recovered(RecoveryAction::ValueReplacedWithMissing),
                    );
                    decoded_row.push(None);
                    continue;
                }
                return Err(Error::UnusableFormula {
                    path: path.to_path_buf(),
                    worksheet: worksheet.map(Into::into),
                    format_candidate: Some(Box::new(detection.format.clone())),
                    row: source_row,
                    column: column + 1,
                    formula: formula.formula.clone().into_boxed_str(),
                    diagnostics: diagnostics.into_boxed_slice(),
                });
            }
            if value.is_empty() || is_missing_token(value) {
                if coordinate {
                    if options.invalid_time_policy == InvalidTimePolicy::Error {
                        return Err(Error::MissingCoordinate {
                            path: path.to_path_buf(),
                            format_candidate: Some(detection.format.clone()),
                            row: source_row,
                            column: column + 1,
                            diagnostics,
                        });
                    }
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::MissingValue,
                            "missing coordinate caused the source row to be skipped",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .recovered(RecoveryAction::RowSkipped),
                    );
                    skip_row = true;
                } else if column < source_cell_count {
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::MissingValue,
                            "missing numeric cell preserved as null",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .with_token(value)
                        .recovered(RecoveryAction::Preserved),
                    );
                }
                decoded_row.push(None);
            } else {
                let parsed_result = value.parse::<f64>();
                let parse_source = parsed_result.as_ref().err().cloned();
                let parsed = parsed_result.ok().filter(|number| number.is_finite());
                if let Some(parsed) = parsed {
                    decoded_row.push(Some(parsed));
                } else if coordinate {
                    if options.invalid_time_policy == InvalidTimePolicy::Error {
                        return Err(Error::MalformedNumeric {
                            path: path.to_path_buf(),
                            format_candidate: Some(detection.format.clone()),
                            row: source_row,
                            column: column + 1,
                            value: value.into(),
                            diagnostics,
                            source: parse_source,
                        });
                    }
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::MalformedTimestamp,
                            "malformed coordinate caused the source row to be skipped",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .with_token(value)
                        .recovered(RecoveryAction::RowSkipped),
                    );
                    decoded_row.push(None);
                    skip_row = true;
                } else if options.invalid_cell_policy == InvalidCellPolicy::Error {
                    return Err(Error::MalformedNumeric {
                        path: path.to_path_buf(),
                        format_candidate: Some(detection.format.clone()),
                        row: source_row,
                        column: column + 1,
                        value: value.into(),
                        diagnostics,
                        source: parse_source,
                    });
                } else {
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::MalformedNumeric,
                            "malformed measurement cell replaced with null",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .with_token(value)
                        .recovered(RecoveryAction::ValueReplacedWithMissing),
                    );
                    diagnostics.push(
                        diagnostic(
                            path,
                            worksheet,
                            &detection.format,
                            options,
                            Severity::Warning,
                            DiagnosticCode::ValueReplacedWithMissing,
                            "invalid value was replaced with a missing cell",
                        )
                        .at(source_row, Some(column + 1))
                        .for_column(column_label(columns, column))
                        .with_token(value)
                        .recovered(RecoveryAction::ValueReplacedWithMissing),
                    );
                    decoded_row.push(None);
                }
            }
        }
        if skip_row {
            if !diagnostics
                .iter()
                .rev()
                .any(|item| item.row == Some(source_row) && item.code == DiagnosticCode::RowSkipped)
            {
                let coordinate_index = columns
                    .iter()
                    .position(|descriptor| is_coordinate(&descriptor.role))
                    .unwrap_or(0);
                diagnostics.push(
                    diagnostic(
                        path,
                        worksheet,
                        &detection.format,
                        options,
                        Severity::Warning,
                        DiagnosticCode::RowSkipped,
                        "invalid coordinate row skipped",
                    )
                    .at(source_row, Some(coordinate_index + 1))
                    .for_column(column_label(columns, coordinate_index))
                    .recovered(RecoveryAction::RowSkipped),
                );
            }
            continue;
        }
        for (column, value) in decoded_row.into_iter().enumerate() {
            values[column].push(value);
        }
        source_rows.push(source_row);
    }
    if values.first().is_none_or(Vec::is_empty) {
        return Err(Error::EmptyData);
    }
    let polars_columns: Vec<Column> = columns
        .iter()
        .zip(&values)
        .enumerate()
        .map(|(index, (descriptor, values))| {
            let name = match naming {
                ColumnNamePolicy::Canonical
                    if options.effective_profile() == crate::ReadProfile::Compatibility
                        && detection.format == crate::FormatId::RegularXy
                        && columns.len() == 2 =>
                {
                    if index == 0 {
                        "x"
                    } else {
                        "y"
                    }
                }
                ColumnNamePolicy::Canonical => &descriptor.canonical_name,
                ColumnNamePolicy::PreserveSource if !descriptor.source_name.is_empty() => {
                    &descriptor.source_name
                }
                ColumnNamePolicy::PreserveSource => &descriptor.canonical_name,
            };
            Column::new(name.into(), values.clone())
        })
        .collect();
    Ok(DecodedTable {
        data: DataFrame::new_infer_height(polars_columns)?,
        values,
        source_rows,
        source_data_row_count,
        diagnostics,
    })
}

fn is_coordinate(role: &ColumnRole) -> bool {
    matches!(role, ColumnRole::Time | ColumnRole::Frequency)
}

fn is_missing_token(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "na" | "n/a" | "nan" | "null" | "missing"
    )
}

fn column_label(columns: &[crate::ColumnDescriptor], index: usize) -> String {
    columns
        .get(index)
        .map(|descriptor| {
            if descriptor.source_name.is_empty() {
                descriptor.canonical_name.clone()
            } else {
                descriptor.source_name.clone()
            }
        })
        .unwrap_or_else(|| format!("column_{}", index + 1))
}

fn diagnostic(
    path: &Path,
    worksheet: Option<&str>,
    format: &crate::FormatId,
    options: &ReadOptions,
    severity: Severity,
    code: DiagnosticCode,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(severity, code, message).with_context(
        path,
        worksheet,
        format,
        options.effective_profile(),
    )
}

fn validate_coordinate_order(
    path: &Path,
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    match dataset.kind() {
        crate::DatasetKind::TimeSeries => validate_time_order(path, dataset, options, diagnostics),
        crate::DatasetKind::ImpedanceSpectrum => {
            validate_frequency_order(path, dataset, options, diagnostics)
        }
        crate::DatasetKind::GenericTable => Ok(()),
    }
}

fn validate_time_order(
    path: &Path,
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let values = dataset.f64_values(&ColumnRole::Time)?;
    let column = dataset
        .columns
        .iter()
        .position(|descriptor| descriptor.role == ColumnRole::Time)
        .unwrap_or(0);
    let mut previous = None;
    for (index, value) in values.iter().copied().enumerate() {
        let Some(value) = value else { continue };
        let Some(before) = previous else {
            previous = Some(value);
            continue;
        };
        let code = if value == before {
            Some(DiagnosticCode::DuplicateTimestamp)
        } else if value < before {
            Some(DiagnosticCode::NonMonotonicTimestamp)
        } else {
            None
        };
        if let Some(code) = code {
            let row = dataset.source_row(index).unwrap_or(index + 1);
            if options.coordinate_order_policy == CoordinateOrderPolicy::Error {
                let common = (
                    path.to_path_buf(),
                    Some(dataset.format.clone()),
                    ColumnRole::Time,
                    row,
                    column + 1,
                    diagnostics.clone(),
                );
                return if code == DiagnosticCode::DuplicateTimestamp {
                    Err(Error::DuplicateCoordinate {
                        path: common.0,
                        format_candidate: common.1,
                        role: common.2,
                        row: common.3,
                        column: common.4,
                        diagnostics: common.5,
                    })
                } else {
                    Err(Error::NonMonotonicCoordinate {
                        path: common.0,
                        format_candidate: common.1,
                        role: common.2,
                        row: common.3,
                        column: common.4,
                        diagnostics: common.5,
                    })
                };
            }
            diagnostics.push(
                diagnostic(
                    path,
                    dataset.metadata.provenance.worksheet.as_deref(),
                    &dataset.format,
                    options,
                    Severity::Warning,
                    code,
                    if code == DiagnosticCode::DuplicateTimestamp {
                        "duplicate timestamp preserved in source order"
                    } else {
                        "decreasing timestamp preserved in source order"
                    },
                )
                .at(row, Some(column + 1))
                .for_column(column_label(&dataset.columns, column))
                .recovered(RecoveryAction::Preserved),
            );
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_frequency_order(
    path: &Path,
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let values = dataset.f64_values(&ColumnRole::Frequency)?;
    let column = dataset
        .columns
        .iter()
        .position(|descriptor| descriptor.role == ColumnRole::Frequency)
        .unwrap_or(0);
    let direction = values.windows(2).find_map(|pair| match (pair[0], pair[1]) {
        (Some(left), Some(right)) if left != right => Some(right.total_cmp(&left)),
        _ => None,
    });
    let mut previous = None;
    for (index, value) in values.iter().copied().enumerate() {
        let Some(value) = value else { continue };
        let row = dataset.source_row(index).unwrap_or(index + 1);
        let invalid = value <= 0.0;
        let duplicate = previous == Some(value);
        let reversal = previous.is_some_and(|before| {
            direction.is_some_and(|expected| value.total_cmp(&before) != expected) && !duplicate
        });
        if invalid || duplicate || reversal {
            if options.coordinate_order_policy == CoordinateOrderPolicy::Error {
                return if duplicate {
                    Err(Error::DuplicateCoordinate {
                        path: path.to_path_buf(),
                        format_candidate: Some(dataset.format.clone()),
                        role: ColumnRole::Frequency,
                        row,
                        column: column + 1,
                        diagnostics: diagnostics.clone(),
                    })
                } else {
                    Err(Error::NonMonotonicCoordinate {
                        path: path.to_path_buf(),
                        format_candidate: Some(dataset.format.clone()),
                        role: ColumnRole::Frequency,
                        row,
                        column: column + 1,
                        diagnostics: diagnostics.clone(),
                    })
                };
            }
            diagnostics.push(
                diagnostic(
                    path,
                    dataset.metadata.provenance.worksheet.as_deref(),
                    &dataset.format,
                    options,
                    if invalid {
                        Severity::Error
                    } else {
                        Severity::Warning
                    },
                    if invalid {
                        DiagnosticCode::NonPositiveFrequency
                    } else {
                        DiagnosticCode::NonMonotonicFrequency
                    },
                    if invalid {
                        "non-positive frequency preserved by compatibility policy"
                    } else {
                        "nonmonotonic frequency preserved in source order"
                    },
                )
                .at(row, Some(column + 1))
                .for_column(column_label(&dataset.columns, column))
                .recovered(RecoveryAction::Preserved),
            );
        }
        previous = Some(value);
    }
    Ok(())
}

struct MetadataContext<'a> {
    path: &'a Path,
    probe: &'a Probe,
    detection: &'a Detection,
    options: &'a ReadOptions,
    descriptors: &'a [crate::ColumnDescriptor],
    formula_cells: &'a [FormulaCell],
    encoding: Option<TextEncoding>,
    source_sha256: String,
}

fn parse_metadata(
    rows: &[Vec<String>],
    raw_rows: &[String],
    context: MetadataContext<'_>,
) -> Metadata {
    let MetadataContext {
        path,
        probe,
        detection,
        options,
        descriptors,
        formula_cells,
        encoding,
        source_sha256,
    } = context;
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
            worksheet_index: probe.worksheet_index,
            delimiter: probe.delimiter.map(char::from),
            encoding,
            detected_format: detection.format.clone(),
            detection_confidence: detection.confidence,
            detection_evidence: detection.evidence.clone(),
            read_options: options.normalized_for_provenance(),
            source_columns: descriptors
                .iter()
                .enumerate()
                .map(|(index, descriptor)| SourceColumnProvenance {
                    position: index + 1,
                    source_name: descriptor.source_name.clone(),
                    canonical_name: descriptor.canonical_name.clone(),
                    role: descriptor.role.clone(),
                    unit: descriptor.unit.clone().unwrap_or(Unit::Unknown),
                })
                .collect(),
            formulas: formula_cells
                .iter()
                .map(|formula| FormulaProvenance {
                    row: formula.row,
                    column: formula.column,
                    formula: formula.formula.clone(),
                    cache_status: if formula.usable_numeric_cache {
                        FormulaCacheStatus::UsableNumeric
                    } else {
                        FormulaCacheStatus::Unusable
                    },
                })
                .collect(),
            source_sha256,
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
