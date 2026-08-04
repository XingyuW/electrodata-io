//! Non-fatal validation and decoding reports.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Severity {
    /// Informational observation.
    Info,
    /// A suspicious condition that did not prevent loading.
    Warning,
    /// A validation error retained as a diagnostic by policy.
    Error,
}

/// Stable machine-readable diagnostic identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DiagnosticCode {
    /// Input was decoded with replacement characters.
    LossyDecoding,
    /// Two or more time values are equal.
    DuplicateTime,
    /// Time decreases within an OCPT series.
    NonMonotonicTime,
    /// Recorded duration differs from declared run time.
    DurationMismatch,
    /// Frequency is not monotonic.
    NonMonotonicFrequency,
    /// Frequency is non-positive.
    NonPositiveFrequency,
    /// Measured impedance magnitude differs from the component-derived value.
    MagnitudeMismatch,
    /// Measured phase differs from the component-derived value.
    PhaseMismatch,
    /// A formula cell lacks a usable cached numeric value.
    UnusableFormula,
}

/// A structured non-fatal report.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Stable code.
    pub code: DiagnosticCode,
    /// Human-readable description.
    pub message: String,
    /// One-based source row when known.
    pub row: Option<usize>,
    /// Source or canonical column name when known.
    pub column: Option<String>,
}
