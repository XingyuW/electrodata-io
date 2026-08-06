//! Structured error types and stable classification.

use crate::formats::FormatId;
use crate::{DatasetKind, DetectionResult, Diagnostic};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

/// Stable high-level category for programmatic error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    Io,
    UnsupportedContainer,
    UnsupportedBinary,
    Encoding,
    Detection,
    Worksheet,
    Schema,
    MalformedNumeric,
    RaggedRow,
    MissingValue,
    CoordinateOrder,
    Unit,
    Formula,
    Metadata,
    Backend,
    Accessor,
}

/// Uniform structured context available for every [`Error`].
#[derive(Debug, Clone, Copy)]
pub struct ErrorContext<'a> {
    pub path: Option<&'a Path>,
    pub worksheet: Option<&'a str>,
    pub format_candidate: Option<&'a FormatId>,
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub category: ErrorCategory,
    pub diagnostics: &'a [Diagnostic],
}

/// Structured EIS header-schema failure retained by [`Error::EisSchemaConflict`].
#[derive(Debug, Clone)]
pub struct EisSchemaError {
    pub path: PathBuf,
    pub format_candidate: FormatId,
    pub row: usize,
    pub column: usize,
    pub header: String,
    pub candidate_roles: Vec<crate::ColumnRole>,
    pub conflicting_role: Option<crate::ColumnRole>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Structured source context for a unit-validation failure.
#[derive(Debug, Clone)]
pub struct UnitErrorContext {
    /// Exact source header when the failure arose from a dataset column.
    pub source_header: Option<String>,
}

/// Structured read failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{source}")]
    ReadContext {
        path: PathBuf,
        worksheet: Option<String>,
        format_candidate: Option<FormatId>,
        diagnostics: Vec<Diagnostic>,
        #[source]
        source: Box<Error>,
    },
    #[error("I/O failure for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported container format for {path}: {container}")]
    UnsupportedContainer { path: PathBuf, container: String },
    #[error("unsupported binary input {path} (magic {magic})")]
    UnsupportedBinary {
        path: PathBuf,
        magic: String,
        format_candidate: Option<FormatId>,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("workbook opening failure for {path}: {source}")]
    WorkbookOpening {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("missing worksheet {worksheet:?} in {path}")]
    MissingWorksheet { path: PathBuf, worksheet: String },
    #[error(
        "unknown scientific format for {path} (best confidence {best_confidence:.3}, threshold {threshold:.3})"
    )]
    UnknownFormat {
        path: PathBuf,
        best_confidence: f32,
        threshold: f32,
    },
    #[error("unknown scientific format for {path}: {evidence:?}")]
    UnknownFormatDetailed {
        path: PathBuf,
        evidence: Vec<String>,
    },
    #[error("ambiguous scientific format for {path}: {candidates:?}")]
    AmbiguousFormat {
        path: PathBuf,
        candidates: Vec<(FormatId, f32)>,
    },
    #[error("ambiguous scientific format detection for {path}: {candidates:?}")]
    AmbiguousDetection {
        path: PathBuf,
        candidates: Vec<DetectionResult>,
    },
    #[error("header not found")]
    HeaderNotFound,
    #[error("ambiguous headerless schema for {path}")]
    AmbiguousHeaderlessSchema {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        diagnostics: Vec<Diagnostic>,
    },
    #[error(
        "ambiguous first table row at source row {row} for {path}: {reason} (policy {policy:?})"
    )]
    AmbiguousFirstRow {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        row: usize,
        reason: String,
        policy: crate::HeaderPolicy,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("invalid column signature: {0}")]
    InvalidColumnSignature(String),
    #[error("invalid column count at row {row}: expected {expected}, found {actual}")]
    InvalidColumnCount {
        row: usize,
        expected: usize,
        actual: usize,
    },
    #[error("malformed numeric value at row {row}, column {column}: {value:?}")]
    MalformedNumeric {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        row: usize,
        column: usize,
        value: String,
        diagnostics: Vec<Diagnostic>,
        #[source]
        source: Option<std::num::ParseFloatError>,
    },
    #[error("ragged row {row}: expected {expected} cells, found {actual}")]
    RaggedRow {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        row: usize,
        expected: usize,
        actual: usize,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("missing required coordinate at row {row}, column {column}")]
    MissingCoordinate {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        row: usize,
        column: usize,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("missing required role {role:?} for {format_candidate} in {path}")]
    MissingRequiredRole {
        path: PathBuf,
        format_candidate: FormatId,
        role: crate::ColumnRole,
        detected_roles: Vec<crate::ColumnRole>,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("EIS schema conflict: {details:?}")]
    EisSchemaConflict { details: Box<EisSchemaError> },
    #[error("duplicate {role:?} coordinate at source row {row}")]
    DuplicateCoordinate {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        role: crate::ColumnRole,
        row: usize,
        column: usize,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("nonmonotonic {role:?} coordinate at source row {row}")]
    NonMonotonicCoordinate {
        path: PathBuf,
        format_candidate: Option<FormatId>,
        role: crate::ColumnRole,
        row: usize,
        column: usize,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("unusable formula cache at row {row}, column {column} in worksheet {worksheet:?}")]
    UnusableFormula {
        path: PathBuf,
        worksheet: Option<Box<str>>,
        format_candidate: Option<Box<FormatId>>,
        row: usize,
        column: usize,
        formula: Box<str>,
        diagnostics: Box<[Diagnostic]>,
    },
    #[error("empty data section")]
    EmptyData,
    #[error("Polars failure: {0}")]
    Polars(#[from] polars::error::PolarsError),
    #[error("Calamine failure for {path}: {source}")]
    Calamine {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("delimited-text decoding failure for {path} at row {row}: {source}")]
    DelimitedText {
        path: PathBuf,
        row: usize,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("metadata parsing failure at row {row}: {message}")]
    Metadata { row: usize, message: String },
    #[error("invalid UTF-8 in {path} at byte {valid_up_to}")]
    InvalidUtf8 {
        path: PathBuf,
        valid_up_to: usize,
        #[source]
        source: std::str::Utf8Error,
    },
    #[error("format handler is not registered: {0}")]
    UnregisteredFormat(FormatId),
    #[error("column role not found: {0:?}")]
    ColumnRoleNotFound(crate::ColumnRole),
    #[error("column role is ambiguous: {role:?} ({matches} matches)")]
    ColumnRoleAmbiguous {
        role: crate::ColumnRole,
        matches: usize,
    },
    #[error("dataset kind {actual:?} cannot be viewed as {expected:?}")]
    WrongDatasetKind {
        expected: DatasetKind,
        actual: DatasetKind,
    },
    #[error("dataset kind {dataset_kind:?} in {path} cannot be viewed as {requested_view}")]
    InvalidDatasetView {
        path: PathBuf,
        requested_view: String,
        dataset_kind: DatasetKind,
    },
    #[error("missing value for {role:?} at dataset row {row}")]
    MissingValue {
        path: PathBuf,
        role: crate::ColumnRole,
        row: usize,
        source_row: Option<usize>,
        column: Option<usize>,
    },
    #[error("unknown source unit for required role {role:?} in {path}")]
    UnknownUnit {
        path: PathBuf,
        role: crate::ColumnRole,
        column: Option<usize>,
        details: Box<UnitErrorContext>,
    },
    #[error("cannot convert {role:?} from {source_unit:?} to {target_unit:?} in {path}")]
    IncompatibleUnit {
        path: PathBuf,
        role: crate::ColumnRole,
        source_unit: crate::Unit,
        target_unit: crate::Unit,
        column: Option<usize>,
        details: Box<UnitErrorContext>,
    },
}

impl Error {
    /// Returns all common context without matching individual error variants.
    pub fn context(&self) -> ErrorContext<'_> {
        ErrorContext {
            path: self.path(),
            worksheet: self.worksheet(),
            format_candidate: self.format_candidate(),
            row: self.row(),
            column: self.column(),
            category: self.category(),
            diagnostics: self.diagnostics(),
        }
    }

    /// Retained underlying parser/backend error, when one exists.
    pub fn source_error(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(self)
    }

    /// Stable error category, independent of display wording.
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::ReadContext { source, .. } => source.category(),
            Self::Io { .. } => ErrorCategory::Io,
            Self::UnsupportedContainer { .. } => ErrorCategory::UnsupportedContainer,
            Self::UnsupportedBinary { .. } => ErrorCategory::UnsupportedBinary,
            Self::InvalidUtf8 { .. } => ErrorCategory::Encoding,
            Self::UnknownFormat { .. }
            | Self::UnknownFormatDetailed { .. }
            | Self::AmbiguousFormat { .. }
            | Self::AmbiguousDetection { .. }
            | Self::UnregisteredFormat(_) => ErrorCategory::Detection,
            Self::WorkbookOpening { .. }
            | Self::MissingWorksheet { .. }
            | Self::Calamine { .. } => ErrorCategory::Worksheet,
            Self::HeaderNotFound
            | Self::AmbiguousHeaderlessSchema { .. }
            | Self::AmbiguousFirstRow { .. }
            | Self::InvalidColumnSignature(_)
            | Self::InvalidColumnCount { .. }
            | Self::MissingRequiredRole { .. }
            | Self::EisSchemaConflict { .. }
            | Self::EmptyData => ErrorCategory::Schema,
            Self::MalformedNumeric { .. } => ErrorCategory::MalformedNumeric,
            Self::RaggedRow { .. } => ErrorCategory::RaggedRow,
            Self::MissingCoordinate { .. } | Self::MissingValue { .. } => {
                ErrorCategory::MissingValue
            }
            Self::DuplicateCoordinate { .. } | Self::NonMonotonicCoordinate { .. } => {
                ErrorCategory::CoordinateOrder
            }
            Self::UnknownUnit { .. } | Self::IncompatibleUnit { .. } => ErrorCategory::Unit,
            Self::UnusableFormula { .. } => ErrorCategory::Formula,
            Self::Metadata { .. } => ErrorCategory::Metadata,
            Self::Polars(_) | Self::DelimitedText { .. } => ErrorCategory::Backend,
            Self::ColumnRoleNotFound(_)
            | Self::ColumnRoleAmbiguous { .. }
            | Self::WrongDatasetKind { .. } => ErrorCategory::Accessor,
            Self::InvalidDatasetView { .. } => ErrorCategory::Accessor,
        }
    }

    /// Physical path retained by this error, when applicable.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::ReadContext { path, .. }
            | Self::Io { path, .. }
            | Self::UnsupportedBinary { path, .. }
            | Self::UnsupportedContainer { path, .. }
            | Self::WorkbookOpening { path, .. }
            | Self::MissingWorksheet { path, .. }
            | Self::Calamine { path, .. }
            | Self::DelimitedText { path, .. }
            | Self::UnknownFormatDetailed { path, .. }
            | Self::UnknownFormat { path, .. }
            | Self::AmbiguousFormat { path, .. }
            | Self::AmbiguousDetection { path, .. }
            | Self::AmbiguousHeaderlessSchema { path, .. }
            | Self::AmbiguousFirstRow { path, .. }
            | Self::MalformedNumeric { path, .. }
            | Self::RaggedRow { path, .. }
            | Self::MissingCoordinate { path, .. }
            | Self::MissingRequiredRole { path, .. }
            | Self::DuplicateCoordinate { path, .. }
            | Self::NonMonotonicCoordinate { path, .. }
            | Self::UnusableFormula { path, .. }
            | Self::MissingValue { path, .. }
            | Self::UnknownUnit { path, .. }
            | Self::IncompatibleUnit { path, .. }
            | Self::InvalidDatasetView { path, .. }
            | Self::InvalidUtf8 { path, .. } => Some(path),
            Self::EisSchemaConflict { details } => Some(&details.path),
            _ => None,
        }
    }

    /// Selected format candidate retained by this error, when known.
    pub fn format_candidate(&self) -> Option<&FormatId> {
        match self {
            Self::ReadContext {
                format_candidate, ..
            }
            | Self::UnsupportedBinary {
                format_candidate, ..
            }
            | Self::AmbiguousHeaderlessSchema {
                format_candidate, ..
            }
            | Self::AmbiguousFirstRow {
                format_candidate, ..
            }
            | Self::MalformedNumeric {
                format_candidate, ..
            }
            | Self::RaggedRow {
                format_candidate, ..
            }
            | Self::MissingCoordinate {
                format_candidate, ..
            }
            | Self::DuplicateCoordinate {
                format_candidate, ..
            }
            | Self::NonMonotonicCoordinate {
                format_candidate, ..
            } => format_candidate.as_ref(),
            Self::UnusableFormula {
                format_candidate, ..
            } => format_candidate.as_deref(),
            Self::MissingRequiredRole {
                format_candidate, ..
            } => Some(format_candidate),
            Self::EisSchemaConflict { details } => Some(&details.format_candidate),
            _ => None,
        }
    }

    /// Selected or requested worksheet retained by this error, when singular.
    pub fn worksheet(&self) -> Option<&str> {
        match self {
            Self::ReadContext { worksheet, .. } => worksheet.as_deref(),
            Self::MissingWorksheet { worksheet, .. } => Some(worksheet),
            Self::UnusableFormula { worksheet, .. } => worksheet.as_deref(),
            _ => None,
        }
    }

    /// One-based source row retained by this error, when known.
    pub fn row(&self) -> Option<usize> {
        match self {
            Self::ReadContext { source, .. } => source.row(),
            Self::AmbiguousFirstRow { row, .. }
            | Self::InvalidColumnCount { row, .. }
            | Self::MalformedNumeric { row, .. }
            | Self::RaggedRow { row, .. }
            | Self::MissingCoordinate { row, .. }
            | Self::DuplicateCoordinate { row, .. }
            | Self::NonMonotonicCoordinate { row, .. }
            | Self::UnusableFormula { row, .. }
            | Self::Metadata { row, .. }
            | Self::DelimitedText { row, .. } => Some(*row),
            Self::MissingValue { source_row, .. } => *source_row,
            Self::EisSchemaConflict { details } => Some(details.row),
            _ => None,
        }
    }

    /// One-based source column retained by this error, when known.
    pub fn column(&self) -> Option<usize> {
        match self {
            Self::ReadContext { source, .. } => source.column(),
            Self::MalformedNumeric { column, .. }
            | Self::MissingCoordinate { column, .. }
            | Self::DuplicateCoordinate { column, .. }
            | Self::NonMonotonicCoordinate { column, .. } => Some(*column),
            Self::UnusableFormula { column, .. } => Some(*column),
            Self::MissingValue { column, .. }
            | Self::UnknownUnit { column, .. }
            | Self::IncompatibleUnit { column, .. } => *column,
            Self::EisSchemaConflict { details } => Some(details.column),
            _ => None,
        }
    }

    /// Diagnostics accumulated before failure.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::ReadContext {
                diagnostics,
                source,
                ..
            } => {
                if diagnostics.is_empty() {
                    source.diagnostics()
                } else {
                    diagnostics
                }
            }
            Self::UnsupportedBinary { diagnostics, .. }
            | Self::AmbiguousHeaderlessSchema { diagnostics, .. }
            | Self::AmbiguousFirstRow { diagnostics, .. }
            | Self::MalformedNumeric { diagnostics, .. }
            | Self::RaggedRow { diagnostics, .. }
            | Self::MissingCoordinate { diagnostics, .. }
            | Self::MissingRequiredRole { diagnostics, .. }
            | Self::DuplicateCoordinate { diagnostics, .. }
            | Self::NonMonotonicCoordinate { diagnostics, .. } => diagnostics,
            Self::EisSchemaConflict { details } => &details.diagnostics,
            Self::UnusableFormula { diagnostics, .. } => diagnostics,
            _ => &[],
        }
    }

    pub(crate) fn with_read_context(
        self,
        path: &Path,
        worksheet: Option<&str>,
        format_candidate: Option<FormatId>,
    ) -> Self {
        if self.path().is_some() {
            self
        } else {
            Self::ReadContext {
                path: path.to_path_buf(),
                worksheet: worksheet.map(str::to_owned),
                format_candidate,
                diagnostics: Vec::new(),
                source: Box::new(self),
            }
        }
    }

    pub(crate) fn with_read_context_and_diagnostics(
        mut self,
        path: &Path,
        worksheet: Option<&str>,
        format_candidate: Option<FormatId>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        if self.path().is_some() {
            if !diagnostics.is_empty() {
                match &mut self {
                    Self::ReadContext {
                        diagnostics: current,
                        ..
                    }
                    | Self::UnsupportedBinary {
                        diagnostics: current,
                        ..
                    }
                    | Self::AmbiguousHeaderlessSchema {
                        diagnostics: current,
                        ..
                    }
                    | Self::AmbiguousFirstRow {
                        diagnostics: current,
                        ..
                    }
                    | Self::MalformedNumeric {
                        diagnostics: current,
                        ..
                    }
                    | Self::RaggedRow {
                        diagnostics: current,
                        ..
                    }
                    | Self::MissingCoordinate {
                        diagnostics: current,
                        ..
                    }
                    | Self::MissingRequiredRole {
                        diagnostics: current,
                        ..
                    }
                    | Self::DuplicateCoordinate {
                        diagnostics: current,
                        ..
                    }
                    | Self::NonMonotonicCoordinate {
                        diagnostics: current,
                        ..
                    } => *current = diagnostics,
                    Self::UnusableFormula {
                        diagnostics: current,
                        ..
                    } => *current = diagnostics.into_boxed_slice(),
                    _ => {}
                }
            }
            return self;
        }
        Self::ReadContext {
            path: path.to_path_buf(),
            worksheet: worksheet.map(str::to_owned),
            format_candidate,
            diagnostics,
            source: Box::new(self),
        }
    }
}
