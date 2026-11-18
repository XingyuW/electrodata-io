//! Reader configuration and explicit recovery policies.

use crate::formats::FormatId;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Named read-policy profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ReadProfile {
    /// Reject malformed coordinates, malformed cells, and ragged rows.
    #[default]
    Strict,
    /// Preserve legacy loadability with fully diagnosed recovery.
    Compatibility,
}

/// Policy for a malformed or missing time/frequency coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InvalidTimePolicy {
    /// Stop at the offending source cell.
    #[default]
    Error,
    /// Skip the complete source row and emit diagnostics.
    SkipRow,
}

/// Policy for a malformed non-coordinate cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InvalidCellPolicy {
    /// Stop at the offending source cell.
    #[default]
    Error,
    /// Retain the row, replace the cell with `None`, and diagnose recovery.
    Null,
}

/// Policy for short or long source rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RaggedRowPolicy {
    /// Stop at the first row whose width differs from the header/table width.
    #[default]
    Error,
    /// Pad short rows with nulls and ignore trailing cells, with diagnostics.
    PadNulls,
}

/// Policy for duplicate or decreasing coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CoordinateOrderPolicy {
    /// Reject duplicate/decreasing time and nonmonotonic frequency.
    #[default]
    Error,
    /// Preserve exact source order and diagnose suspicious transitions.
    Preserve,
}

/// Policy for interpreting a regular table's first row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HeaderPolicy {
    /// Infer the policy from content; ambiguous headerless data is strict-mode error.
    #[default]
    Auto,
    /// Require a header.
    Present,
    /// Treat the first row as data and explicitly permit headerless inference.
    Absent,
}

/// Policy for names exposed by the compatibility DataFrame.
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
#[non_exhaustive]
pub struct ReadOptions {
    pub detection_threshold: f32,
    pub detection_margin: f32,
    pub forced_format: Option<FormatId>,
    pub header_policy: HeaderPolicy,
    pub column_name_policy: ColumnNamePolicy,
    pub sheet: SheetSelector,
    pub validation: ValidationLevel,
    pub tolerances: ValidationTolerances,
    pub probe_rows: usize,
    pub allow_lossy_utf8: bool,
    /// Named profile recorded for diagnostics/provenance.
    pub profile: ReadProfile,
    /// Malformed/missing coordinate policy.
    pub invalid_time_policy: InvalidTimePolicy,
    /// Malformed measurement-cell policy.
    pub invalid_cell_policy: InvalidCellPolicy,
    /// Ragged-row policy.
    pub ragged_row_policy: RaggedRowPolicy,
    /// Duplicate/nonmonotonic coordinate policy.
    pub coordinate_order_policy: CoordinateOrderPolicy,
    /// Field order of numeric `a/b/yyyy` dates the column itself cannot settle.
    pub date_order: Option<crate::DateOrder>,
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
            profile: ReadProfile::Strict,
            invalid_time_policy: InvalidTimePolicy::Error,
            invalid_cell_policy: InvalidCellPolicy::Error,
            ragged_row_policy: RaggedRowPolicy::Error,
            coordinate_order_policy: CoordinateOrderPolicy::Error,
            date_order: None,
        }
    }
}

impl ReadOptions {
    /// Creates strict default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates the explicit compatibility profile.
    pub fn compatibility() -> Self {
        Self {
            profile: ReadProfile::Compatibility,
            invalid_time_policy: InvalidTimePolicy::SkipRow,
            invalid_cell_policy: InvalidCellPolicy::Null,
            ragged_row_policy: RaggedRowPolicy::PadNulls,
            coordinate_order_policy: CoordinateOrderPolicy::Preserve,
            ..Self::default()
        }
    }

    pub fn with_profile(mut self, profile: ReadProfile) -> Self {
        self.profile = profile;
        match profile {
            ReadProfile::Strict => {
                self.invalid_time_policy = InvalidTimePolicy::Error;
                self.invalid_cell_policy = InvalidCellPolicy::Error;
                self.ragged_row_policy = RaggedRowPolicy::Error;
                self.coordinate_order_policy = CoordinateOrderPolicy::Error;
            }
            ReadProfile::Compatibility => {
                self.invalid_time_policy = InvalidTimePolicy::SkipRow;
                self.invalid_cell_policy = InvalidCellPolicy::Null;
                self.ragged_row_policy = RaggedRowPolicy::PadNulls;
                self.coordinate_order_policy = CoordinateOrderPolicy::Preserve;
            }
        }
        self
    }

    pub fn with_invalid_time_policy(mut self, policy: InvalidTimePolicy) -> Self {
        self.invalid_time_policy = policy;
        self.synchronize_profile();
        self
    }

    pub fn with_invalid_cell_policy(mut self, policy: InvalidCellPolicy) -> Self {
        self.invalid_cell_policy = policy;
        self.synchronize_profile();
        self
    }

    pub fn with_ragged_row_policy(mut self, policy: RaggedRowPolicy) -> Self {
        self.ragged_row_policy = policy;
        self.synchronize_profile();
        self
    }

    pub fn with_coordinate_order_policy(mut self, policy: CoordinateOrderPolicy) -> Self {
        self.coordinate_order_policy = policy;
        self.synchronize_profile();
        self
    }

    /// States the field order of numeric `a/b/yyyy` dates.
    pub fn with_date_order(mut self, order: crate::DateOrder) -> Self {
        self.date_order = Some(order);
        self
    }

    pub fn with_format(mut self, format: FormatId) -> Self {
        self.forced_format = Some(format);
        self
    }

    pub fn with_header_policy(mut self, policy: HeaderPolicy) -> Self {
        self.header_policy = policy;
        self
    }

    pub fn with_column_name_policy(mut self, policy: ColumnNamePolicy) -> Self {
        self.column_name_policy = policy;
        self
    }

    pub fn with_sheet(mut self, selector: SheetSelector) -> Self {
        self.sheet = selector;
        self
    }

    pub fn with_validation(mut self, level: ValidationLevel) -> Self {
        self.validation = level;
        self
    }

    pub(crate) fn permits_headerless_inference(&self) -> bool {
        self.header_policy == HeaderPolicy::Absent
            || self.effective_profile() == ReadProfile::Compatibility
            || self.invalid_time_policy != InvalidTimePolicy::Error
            || self.invalid_cell_policy != InvalidCellPolicy::Error
            || self.ragged_row_policy != RaggedRowPolicy::Error
    }

    pub(crate) fn effective_profile(&self) -> ReadProfile {
        if self.invalid_time_policy != InvalidTimePolicy::Error
            || self.invalid_cell_policy != InvalidCellPolicy::Error
            || self.ragged_row_policy != RaggedRowPolicy::Error
            || self.coordinate_order_policy != CoordinateOrderPolicy::Error
        {
            ReadProfile::Compatibility
        } else {
            ReadProfile::Strict
        }
    }

    pub(crate) fn normalized_for_provenance(&self) -> Self {
        let mut normalized = self.clone();
        normalized.profile = self.effective_profile();
        normalized
    }

    fn synchronize_profile(&mut self) {
        self.profile = self.effective_profile();
    }
}
