//! Semantic column schema.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Scientific role of a column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ColumnRole {
    /// Generic independent variable.
    X,
    /// Generic dependent variable.
    Y,
    /// Time.
    Time,
    /// Conventional single potential.
    Potential,
    /// Numbered potential channel.
    PotentialChannel(u32),
    /// Frequency.
    Frequency,
    /// Real impedance.
    ImpedanceReal,
    /// Imaginary impedance with the source sign convention preserved.
    ImpedanceImaginary,
    /// Measured impedance magnitude.
    ImpedanceMagnitude,
    /// Measured phase.
    Phase,
    /// Handler-defined role.
    Custom(String),
}

/// Parsed unit associated with a column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Unit {
    /// Seconds.
    Second,
    /// Volts.
    Volt,
    /// Hertz.
    Hertz,
    /// Ohms.
    Ohm,
    /// Degrees.
    Degree,
    /// Unrecognized unit, preserved verbatim.
    Other(String),
}

/// Mapping from an original heading to the stable schema.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ColumnDescriptor {
    /// Original source heading, or an empty string for headerless data.
    pub source_name: String,
    /// Stable canonical name.
    pub canonical_name: String,
    /// Semantic role.
    pub role: ColumnRole,
    /// Parsed unit.
    pub unit: Option<Unit>,
}
