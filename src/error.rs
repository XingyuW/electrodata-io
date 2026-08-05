//! Error types.

use crate::DetectionResult;
use crate::formats::FormatId;
use std::path::PathBuf;

/// Crate result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Structured read failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Filesystem I/O failed.
    #[error("I/O failure for {path}: {source}")]
    Io {
        /// Path whose operation failed.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// The storage container is unsupported or its feature is disabled.
    #[error("unsupported container format: {0}")]
    UnsupportedContainer(String),
    /// A workbook could not be opened.
    #[error("workbook opening failure for {path}: {message}")]
    WorkbookOpening {
        /// Workbook path.
        path: PathBuf,
        /// Reader-provided failure detail.
        message: String,
    },
    /// The requested worksheet does not exist.
    #[error("missing worksheet: {0}")]
    MissingWorksheet(String),
    /// No scientific handler confidently recognized the input.
    #[error(
        "unknown scientific format (best confidence {best_confidence:.3}, threshold {threshold:.3})"
    )]
    UnknownFormat {
        /// Strongest confidence returned by any registered handler.
        best_confidence: f32,
        /// Configured minimum confidence.
        threshold: f32,
    },
    /// No built-in format had adequate evidence for a bounded source preview.
    #[error("unknown scientific format for {path}: {evidence:?}")]
    UnknownFormatDetailed {
        /// Source path.
        path: PathBuf,
        /// Evidence collected while probing the source.
        evidence: Vec<String>,
    },
    /// Scientific handlers or worksheets were too close to choose safely.
    #[error("ambiguous scientific format: {candidates:?}")]
    AmbiguousFormat {
        /// Leading candidates and their confidence values.
        candidates: Vec<(FormatId, f32)>,
    },
    /// Multiple built-in formats had similarly strong content evidence.
    #[error("ambiguous scientific format detection: {candidates:?}")]
    AmbiguousDetection {
        /// Leading scientific-format candidates and their evidence.
        candidates: Vec<DetectionResult>,
    },
    /// A required table header was not found.
    #[error("header not found")]
    HeaderNotFound,
    /// Header roles do not form a valid signature.
    #[error("invalid column signature: {0}")]
    InvalidColumnSignature(String),
    /// A row has an unexpected number of columns.
    #[error("invalid column count at row {row}: expected {expected}, found {actual}")]
    InvalidColumnCount {
        /// One-based source row.
        row: usize,
        /// Required width.
        expected: usize,
        /// Observed width.
        actual: usize,
    },
    /// A numeric cell could not be parsed.
    #[error("malformed numeric value at row {row}, column {column}: {value:?}")]
    MalformedNumeric {
        /// One-based source row.
        row: usize,
        /// One-based source column.
        column: usize,
        /// Original cell text.
        value: String,
    },
    /// A row is ragged.
    #[error("ragged row {row}: expected {expected} cells, found {actual}")]
    RaggedRow {
        /// One-based source row.
        row: usize,
        /// Required width.
        expected: usize,
        /// Observed width.
        actual: usize,
    },
    /// The detected data section has no rows.
    #[error("empty data section")]
    EmptyData,
    /// Polars failed while constructing or reading a frame.
    #[error("Polars failure: {0}")]
    Polars(#[from] polars::error::PolarsError),
    /// Calamine failed while reading a workbook.
    #[error("Calamine failure: {0}")]
    Calamine(String),
    /// Required metadata could not be parsed.
    #[error("metadata parsing failure at row {row}: {message}")]
    Metadata {
        /// One-based source row, or zero when unavailable.
        row: usize,
        /// Parser-provided detail.
        message: String,
    },
    /// Strict UTF-8 decoding failed.
    #[error("invalid UTF-8 at byte {valid_up_to}")]
    InvalidUtf8 {
        /// Byte offset up to which decoding was valid.
        valid_up_to: usize,
    },
    /// A forced handler is not registered.
    #[error("format handler is not registered: {0}")]
    UnregisteredFormat(FormatId),
    /// An accessor could not find the requested role.
    #[error("column role not found: {0:?}")]
    ColumnRoleNotFound(crate::schema::ColumnRole),
}
