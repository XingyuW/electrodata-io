//! Typed, raw, and provenance metadata.

use crate::source::ContainerFormat;
use chrono::NaiveDateTime;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Typed acquisition fields common to CH Instruments exports.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AcquisitionMetadata {
    /// Naive local date and time exactly because the source has no timezone.
    pub recorded_at: Option<NaiveDateTime>,
    /// Technique label.
    pub technique: Option<String>,
    /// Declared source file.
    pub source_file: Option<String>,
    /// Declared data source.
    pub data_source: Option<String>,
    /// Instrument model.
    pub instrument_model: Option<String>,
    /// Header field.
    pub header: Option<String>,
    /// Note field.
    pub note: Option<String>,
}

/// Parsed scalar metadata value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MetadataValue {
    /// Signed integer.
    Integer(i64),
    /// Double-precision number.
    Float(f64),
    /// Text or a bare flag.
    Text(String),
}

/// Parameter retaining both its source spelling and parsed representation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Parameter {
    /// Parsed name without a trailing parenthesized unit.
    pub name: String,
    /// Original name exactly as trimmed.
    pub raw_name: String,
    /// Parenthesized unit text, when present.
    pub unit: Option<String>,
    /// Safely parsed typed value.
    pub value: MetadataValue,
    /// Original value exactly as trimmed.
    pub raw_value: String,
    /// One-based source row.
    pub row_number: usize,
}

/// A metadata row preserved without interpretation loss.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RawMetadataRow {
    /// One-based source row.
    pub row_number: usize,
    /// Parsed cells.
    pub cells: Vec<String>,
    /// Original row text without its line ending or BOM.
    pub reconstructed_text: String,
}

/// Physical origin and selected container member.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Provenance {
    /// Caller-provided path.
    pub path: String,
    /// Basename when available.
    pub source_name: String,
    /// Storage container.
    pub container: ContainerFormat,
    /// Selected workbook sheet.
    pub worksheet: Option<String>,
}

/// Metadata retained in typed and raw forms.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Metadata {
    /// Typed acquisition fields.
    pub acquisition: AcquisitionMetadata,
    /// Parsed parameters and bare flags.
    pub parameters: Vec<Parameter>,
    /// Every row preceding the table header or data start.
    pub raw_rows: Vec<RawMetadataRow>,
    /// Physical origin.
    pub provenance: Provenance,
}
