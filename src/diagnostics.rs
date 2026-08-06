//! Structured non-fatal validation and recovery reports.

use crate::{FormatId, ReadProfile};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Collection type used by datasets and errors.
pub type Diagnostics = Vec<Diagnostic>;

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Recovery action taken after the reported condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RecoveryAction {
    #[default]
    None,
    RowSkipped,
    ValueReplacedWithMissing,
    RowPadded,
    ExtraCellsIgnored,
    Preserved,
    RolesInferred,
    LossyDecoded,
}

/// Stable machine-readable diagnostic identifier.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DiagnosticCode {
    LossyDecoding,
    MissingValue,
    MalformedTimestamp,
    MalformedNumeric,
    RaggedRow,
    RowSkipped,
    ValueReplacedWithMissing,
    RowPadded,
    ExtraCellsIgnored,
    HeaderlessRoleInference,
    UnknownUnit,
    DuplicateTimestamp,
    NonMonotonicTimestamp,
    /// Legacy spelling retained for compatibility.
    DuplicateTime,
    /// Legacy spelling retained for compatibility.
    NonMonotonicTime,
    DurationMismatch,
    NonMonotonicFrequency,
    NonPositiveFrequency,
    MagnitudeMismatch,
    PhaseMismatch,
    UnusableFormula,
}

impl PartialEq for DiagnosticCode {
    fn eq(&self, other: &Self) -> bool {
        use DiagnosticCode::{
            DuplicateTime, DuplicateTimestamp, NonMonotonicTime, NonMonotonicTimestamp,
        };
        matches!(
            (self, other),
            (
                DuplicateTime | DuplicateTimestamp,
                DuplicateTime | DuplicateTimestamp
            ) | (
                NonMonotonicTime | NonMonotonicTimestamp,
                NonMonotonicTime | NonMonotonicTimestamp
            )
        ) || std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Eq for DiagnosticCode {}

/// A source-located, machine-readable report.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    /// One-based physical source row.
    pub row: Option<usize>,
    /// Source header or stable column name.
    pub column: Option<String>,
    /// One-based physical source column.
    pub column_index: Option<usize>,
    /// Recovery performed, if any.
    pub recovery: RecoveryAction,
    /// Bounded offending token, when relevant.
    pub token: Option<String>,
    /// Physical path, when available.
    pub path: Option<String>,
    /// Workbook sheet, when available.
    pub worksheet: Option<String>,
    /// Selected scientific format.
    pub format_candidate: Option<FormatId>,
    /// Active read profile.
    pub profile: ReadProfile,
}

impl Diagnostic {
    pub(crate) fn new(
        severity: Severity,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            row: None,
            column: None,
            column_index: None,
            recovery: RecoveryAction::None,
            token: None,
            path: None,
            worksheet: None,
            format_candidate: None,
            profile: ReadProfile::Strict,
        }
    }

    pub(crate) fn at(mut self, row: usize, column_index: Option<usize>) -> Self {
        self.row = Some(row);
        self.column_index = column_index;
        self
    }

    pub(crate) fn for_column(mut self, column: impl Into<String>) -> Self {
        self.column = Some(column.into());
        self
    }

    pub(crate) fn recovered(mut self, recovery: RecoveryAction) -> Self {
        self.recovery = recovery;
        self
    }

    pub(crate) fn with_token(mut self, token: &str) -> Self {
        self.token = Some(token.chars().take(128).collect());
        self
    }

    pub(crate) fn with_context(
        mut self,
        path: &std::path::Path,
        worksheet: Option<&str>,
        format: &FormatId,
        profile: ReadProfile,
    ) -> Self {
        self.path = Some(path.display().to_string());
        self.worksheet = worksheet.map(str::to_owned);
        self.format_candidate = Some(format.clone());
        self.profile = profile;
        self
    }
}

/// Directly available aggregate counts for a decoded dataset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiagnosticSummary {
    pub total_source_rows: usize,
    pub retained_rows: usize,
    pub skipped_rows: usize,
    pub malformed_cells: usize,
    pub malformed_rows: usize,
    pub missing_cells: usize,
    pub missing_by_column: Vec<ColumnMissingCount>,
    pub duplicate_coordinates: usize,
    pub nonmonotonic_transitions: usize,
}

/// Missing-value count for one exact source-position column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMissingCount {
    /// One-based source position.
    pub position: usize,
    /// Stable canonical column name.
    pub name: String,
    /// Semantic role; duplicate roles remain distinct by position.
    pub role: crate::ColumnRole,
    /// Number of retained null cells.
    pub missing_cells: usize,
}
