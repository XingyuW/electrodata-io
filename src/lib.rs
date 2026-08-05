#![cfg_attr(not(any(feature = "csv", feature = "excel")), allow(dead_code))]
//! Extensible, lossless readers for electrochemical and scientific tabular data.
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
mod validation;

pub use dataset::Dataset;
pub use detection::{DetectionResult, ElectroDataFormat};
pub use diagnostics::{Diagnostic, DiagnosticCode, Severity};
pub use error::{Error, Result};
pub use formats::{DetectedLayout, Detection, FormatHandler, FormatId};
pub use metadata::{
    AcquisitionMetadata, Metadata, MetadataValue, Parameter, Provenance, RawMetadataRow,
};
pub use options::{
    ColumnNamePolicy, HeaderPolicy, ReadOptions, SheetSelector, ValidationLevel,
    ValidationTolerances,
};
pub use reader::{DataReader, DataReaderBuilder};
pub use schema::{ColumnDescriptor, ColumnRole, Unit};
pub use source::{ContainerFormat, Probe};

use std::path::Path;

/// Backwards-compatible name for the crate's structured error type.
pub type ElectroDataError = Error;

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
    let path = path.as_ref();
    let detection = detect_format(path)?;
    read_as(path, detection.format)
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

/// Reads a regular two-column scientific table directly.
pub fn read_regular<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_as(path, ElectroDataFormat::Regular)
}

/// Reads a CHI open-circuit-potential table directly.
pub fn read_chi_ocpt<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_as(path, ElectroDataFormat::ChiOcpt)
}

/// Reads a CHI numbered-channel open-circuit-potential table directly.
pub fn read_chi_multichannel_ocpt<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_as(path, ElectroDataFormat::ChiMultichannelOcpt)
}

/// Reads a CHI A.C. impedance table directly.
pub fn read_chi_eis<P>(path: P) -> std::result::Result<Dataset, ElectroDataError>
where
    P: AsRef<Path>,
{
    read_as(path, ElectroDataFormat::ChiEis)
}

/// Reads a supported file with explicit options.
pub fn read_with_options(path: impl AsRef<Path>, options: &ReadOptions) -> Result<Dataset> {
    DataReader::builder()
        .with_builtin_formats()
        .build()
        .read_with_options(path, options)
}
