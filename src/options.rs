//! Reader configuration.

use crate::formats::FormatId;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Policy for interpreting a regular table's first row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HeaderPolicy {
    /// Infer the policy from content.
    #[default]
    Auto,
    /// Require a header.
    Present,
    /// Treat the first row as data.
    Absent,
}

/// Policy for names exposed by the DataFrame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ColumnNamePolicy {
    /// Use stable semantic names.
    #[default]
    Canonical,
    /// Keep the exact trimmed source header.
    PreserveSource,
}

/// Worksheet selection strategy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SheetSelector {
    /// Probe all eligible nonempty sheets.
    #[default]
    Auto,
    /// Select a worksheet by name.
    Name(String),
    /// Select a worksheet by zero-based index.
    Index(usize),
}

/// Validation depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ValidationLevel {
    /// Skip validation after parsing.
    None,
    /// Check table integrity and required roles.
    #[default]
    Structural,
    /// Also run non-mutating domain checks.
    Scientific,
}

/// Numerical tolerances used by scientific validation.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValidationTolerances {
    /// Relative tolerance for EIS magnitude.
    pub eis_magnitude_relative: f64,
    /// Absolute tolerance in degrees for EIS phase.
    pub eis_phase_degrees: f64,
    /// Relative tolerance for duration comparisons.
    pub duration_relative: f64,
}

impl Default for ValidationTolerances {
    fn default() -> Self {
        Self {
            eis_magnitude_relative: 0.01,
            eis_phase_degrees: 0.5,
            duration_relative: 0.01,
        }
    }
}

/// Complete read configuration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ReadOptions {
    /// Minimum winning detection confidence.
    pub detection_threshold: f32,
    /// Minimum confidence margin over the runner-up.
    pub detection_margin: f32,
    /// Optional format override.
    pub forced_format: Option<FormatId>,
    /// Regular-table header policy.
    pub header_policy: HeaderPolicy,
    /// DataFrame naming policy.
    pub column_name_policy: ColumnNamePolicy,
    /// Worksheet selection.
    pub sheet: SheetSelector,
    /// Validation depth.
    pub validation: ValidationLevel,
    /// Domain validation tolerances.
    pub tolerances: ValidationTolerances,
    /// Maximum rows retained by a structural probe.
    pub probe_rows: usize,
    /// Permit replacement-character decoding and emit a warning.
    pub allow_lossy_utf8: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            detection_threshold: 0.65,
            detection_margin: 0.10,
            forced_format: None,
            header_policy: HeaderPolicy::Auto,
            column_name_policy: ColumnNamePolicy::Canonical,
            sheet: SheetSelector::Auto,
            validation: ValidationLevel::Structural,
            tolerances: ValidationTolerances::default(),
            probe_rows: 256,
            allow_lossy_utf8: false,
        }
    }
}

impl ReadOptions {
    /// Creates the default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forces a scientific format.
    pub fn with_format(mut self, format: FormatId) -> Self {
        self.forced_format = Some(format);
        self
    }

    /// Sets the header policy.
    pub fn with_header_policy(mut self, policy: HeaderPolicy) -> Self {
        self.header_policy = policy;
        self
    }

    /// Sets the DataFrame column-name policy.
    pub fn with_column_name_policy(mut self, policy: ColumnNamePolicy) -> Self {
        self.column_name_policy = policy;
        self
    }

    /// Selects a worksheet.
    pub fn with_sheet(mut self, selector: SheetSelector) -> Self {
        self.sheet = selector;
        self
    }

    /// Sets validation depth.
    pub fn with_validation(mut self, level: ValidationLevel) -> Self {
        self.validation = level;
        self
    }
}
