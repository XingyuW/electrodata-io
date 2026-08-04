//! Extensible, lossless readers for electrochemical and scientific tabular data.
//!
//! [`read`] is the convenient entry point. Use [`DataReader`] when registering
//! an application- or instrument-specific [`FormatHandler`].

mod dataset;
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

/// Reads a supported file using [`ReadOptions::default`].
pub fn read(path: impl AsRef<Path>) -> Result<Dataset> {
    DataReader::builder()
        .with_builtin_formats()
        .build()
        .read(path)
}

/// Reads a supported file with explicit options.
pub fn read_with_options(path: impl AsRef<Path>, options: &ReadOptions) -> Result<Dataset> {
    DataReader::builder()
        .with_builtin_formats()
        .build()
        .read_with_options(path, options)
}
