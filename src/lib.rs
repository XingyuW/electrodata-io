#![cfg_attr(not(any(feature = "csv", feature = "excel")), allow(dead_code))]
//! Canonical input and physical-data representations for electrochemical tables.
//!
//! [`read`] is the convenient entry point. Use [`DataReader`] when registering
//! an application- or instrument-specific [`FormatHandler`].

mod dataset;
mod detection;
mod diagnostics;
mod error;
mod formats;
mod metadata;
mod options;
mod reader;
mod schema;
mod source;
mod timestamp;
mod validation;

pub use dataset::{Dataset, EisView, TimeSeriesView};
pub use detection::{DetectionResult, ElectroDataFormat};
pub use diagnostics::{
    ColumnMissingCount, Diagnostic, DiagnosticCode, DiagnosticSummary, Diagnostics, RecoveryAction,
    Severity,
};
pub use error::{
    EisSchemaError, Error, ErrorCategory, ErrorContext, Result, UnitErrorContext,
    WorksheetCandidate,
};
pub use formats::{DetectedLayout, Detection, FormatHandler, FormatId};
pub use metadata::{
    AcquisitionMetadata, FormulaCacheStatus, FormulaProvenance, Metadata, MetadataValue, Parameter,
    Provenance, RawMetadataRow, SourceColumnProvenance, TextEncoding,
};
pub use options::{
    ColumnNamePolicy, CoordinateOrderPolicy, HeaderPolicy, InvalidCellPolicy, InvalidTimePolicy,
    RaggedRowPolicy, ReadOptions, ReadProfile, SheetSelector, ValidationLevel,
    ValidationTolerances,
};
pub use reader::{DataReader, DataReaderBuilder};
pub use schema::{
    Column, ColumnDescriptor, ColumnRole, ConvertedValues, DatasetKind, DatasetSchema, Unit,
    UnitConversion,
};
pub use source::{ContainerFormat, Probe};
pub use timestamp::{DateOrder, TimeOrigin, TimestampFormat};

use std::path::Path;

/// Backwards-compatible name for the crate's structured error type.
pub type ElectroDataError = Error;

/// Canonical name for the crate's retained source metadata.
pub type DatasetMetadata = Metadata;

/// Detects a built-in scientific format from a bounded source preview.
///
/// CSV detection reads only the first 160 physical rows. Workbook detection
/// applies the same content checks to the worksheet candidates used by the
/// Excel reader.
pub fn detect_format<P>(path: P) -> std::result::Result<DetectionResult, ElectroDataError>
where
    P: AsRef<Path>,
{
    DataReader::builder()
        .with_builtin_formats()
        .build()
        .detect_format(path)
}

/// Reads a supported built-in format after content-based automatic detection.
///
/// Use [`read_as`] when the caller already knows the format and wants a direct,
/// deterministic reader choice.
pub fn read<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_with_options(path, &ReadOptions::new())
}

/// Reads a file with an explicitly selected built-in scientific format.
///
/// The selected handler still verifies that the source has a compatible
/// structure; this function never falls back to another format.
pub fn read_as<P>(
    path: P,
    format: ElectroDataFormat,
) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    let options = ReadOptions::new().with_format(format.into());
    read_with_options(path, &options)
}

/// Reads a generalized regular table through the legacy compatibility entry point.
pub fn read_regular<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    let options = ReadOptions::compatibility().with_format(FormatId::RegularXy);
    read_with_options(path, &options)
}

/// Reads a CHI open-circuit-potential table directly.
pub fn read_chi_ocpt<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_with_options(
        path,
        &ReadOptions::compatibility().with_format(FormatId::ChiOcpt),
    )
}

/// Reads a CHI numbered-channel open-circuit-potential table directly.
pub fn read_chi_multichannel_ocpt<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_with_options(
        path,
        &ReadOptions::compatibility().with_format(FormatId::ChiMultichannelOcpt),
    )
}

/// Reads a CHI A.C. impedance table directly.
pub fn read_chi_eis<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_with_options(
        path,
        &ReadOptions::compatibility().with_format(FormatId::ChiEis),
    )
}

/// Reads a supported file with explicit options.
pub fn read_with_options(path: impl AsRef<Path>, options: &ReadOptions) -> Result<Dataset> {
    DataReader::builder()
        .with_builtin_formats()
        .build()
        .read_with_options(path, options)
}
