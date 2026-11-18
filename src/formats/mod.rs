//! Scientific format handlers and built-ins.

mod chi;
mod regular_xy;

use crate::{ColumnDescriptor, Probe, ReadOptions};
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub(crate) use chi::{
    ChiEisHandler, ChiMultichannelOcptHandler, ChiOcptHandler, validate_eis_schema,
};
pub(crate) use regular_xy::{
    FirstRowClassification, RegularXyHandler, classify_first_row, explicit_comment_row, header_role,
};

/// Stable identifier for a scientific format, independent of storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FormatId {
    /// Generic consistently shaped two-column numeric table.
    RegularXy,
    /// CH Instruments conventional open-circuit potential versus time.
    ChiOcpt,
    /// CH Instruments numbered-channel open-circuit potential versus time.
    ChiMultichannelOcpt,
    /// CH Instruments A.C. impedance export.
    ChiEis,
    /// User-defined identifier.
    Custom(String),
}

impl fmt::Display for FormatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegularXy => f.write_str("regular-xy"),
            Self::ChiOcpt => f.write_str("chi-ocpt"),
            Self::ChiMultichannelOcpt => f.write_str("chi-multichannel-ocpt"),
            Self::ChiEis => f.write_str("chi-eis"),
            Self::Custom(value) => f.write_str(value),
        }
    }
}

/// Rows and shape discovered without loading the complete table.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DetectedLayout {
    /// Zero-based header row.
    pub header_row: Option<usize>,
    /// Zero-based first numeric row.
    pub data_start_row: usize,
    /// Half-open zero-based metadata range.
    pub metadata_range: Option<std::ops::Range<usize>>,
    /// Required table width.
    pub expected_column_count: usize,
    /// Whether a source header is present.
    pub has_header: bool,
}

/// Evidence returned by a scientific format handler.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Detection {
    /// Detected format.
    pub format: FormatId,
    /// Confidence in `[0, 1]`.
    pub confidence: f32,
    /// Human-readable evidence.
    pub evidence: Vec<String>,
    /// Detected table layout.
    pub layout: DetectedLayout,
}

/// Object-safe extension point for scientific formats.
pub trait FormatHandler: Send + Sync {
    /// Identifier produced by this handler.
    fn format_id(&self) -> FormatId;
    /// Examines only the bounded probe and returns a candidate when structurally plausible.
    fn detect(&self, probe: &Probe, options: &ReadOptions) -> Option<Detection>;
    /// Produces the semantic schema for a selected detection.
    fn columns(&self, probe: &Probe, detection: &Detection)
    -> crate::Result<Vec<ColumnDescriptor>>;
}

pub(crate) fn trim_cell(value: &str) -> &str {
    value.trim().trim_start_matches('\u{feff}')
}

pub(crate) fn numeric(value: &str) -> bool {
    let value = trim_cell(value);
    !value.is_empty() && value.parse::<f64>().is_ok()
}
