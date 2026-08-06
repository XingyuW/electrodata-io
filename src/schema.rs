//! Stable scientific dataset and column schema.

use std::hash::{Hash, Hasher};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Scientific kind of a dataset, independent of its source container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DatasetKind {
    /// One time coordinate and one or more aligned measurements.
    TimeSeries,
    /// Frequency and complex impedance components.
    ImpedanceSpectrum,
    /// A numeric table whose scientific roles are not fully known.
    GenericTable,
}

/// Scientific role of a column.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum ColumnRole {
    /// Generic independent variable retained for source compatibility.
    X,
    /// Generic dependent variable retained for source compatibility.
    Y,
    /// Time coordinate.
    Time,
    /// Frequency coordinate.
    Frequency,
    /// Real impedance component.
    ImpedanceReal,
    /// Imaginary impedance component, with source sign preserved.
    ImpedanceImaginary,
    /// Measured impedance magnitude.
    ImpedanceMagnitude,
    /// Measured impedance phase.
    ImpedancePhase,
    /// Conventional single potential.
    Potential,
    /// Conventional current.
    Current,
    /// Stable numbered measurement channel.
    MeasurementChannel(u32),
    /// Unresolved scientific role.
    Unknown,
    /// Legacy alias of [`ColumnRole::MeasurementChannel`].
    PotentialChannel(u32),
    /// Legacy alias of [`ColumnRole::ImpedancePhase`].
    Phase,
    /// Handler-defined legacy role.
    Custom(String),
}

impl ColumnRole {
    fn equivalence_tag(&self) -> u8 {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Time => 2,
            Self::Frequency => 3,
            Self::ImpedanceReal => 4,
            Self::ImpedanceImaginary => 5,
            Self::ImpedanceMagnitude => 6,
            Self::ImpedancePhase | Self::Phase => 7,
            Self::Potential => 8,
            Self::Current => 9,
            Self::MeasurementChannel(_) | Self::PotentialChannel(_) => 10,
            Self::Unknown => 11,
            Self::Custom(_) => 12,
        }
    }
}

impl PartialEq for ColumnRole {
    fn eq(&self, other: &Self) -> bool {
        if self.equivalence_tag() != other.equivalence_tag() {
            return false;
        }
        match (self, other) {
            (Self::MeasurementChannel(left), Self::MeasurementChannel(right))
            | (Self::MeasurementChannel(left), Self::PotentialChannel(right))
            | (Self::PotentialChannel(left), Self::MeasurementChannel(right))
            | (Self::PotentialChannel(left), Self::PotentialChannel(right)) => left == right,
            (Self::Custom(left), Self::Custom(right)) => left == right,
            _ => true,
        }
    }
}

impl Eq for ColumnRole {}

impl Hash for ColumnRole {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.equivalence_tag().hash(state);
        match self {
            Self::MeasurementChannel(channel) | Self::PotentialChannel(channel) => {
                channel.hash(state);
            }
            Self::Custom(value) => value.hash(state),
            _ => {}
        }
    }
}

/// Parsed unit associated with a column. Values are never implicitly rescaled.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Unit {
    /// Unit could not be established from the source.
    Unknown,
    /// Seconds.
    Second,
    /// Hours.
    Hour,
    /// Days.
    Day,
    /// Hertz.
    Hertz,
    /// Ohms.
    Ohm,
    /// Degrees.
    Degree,
    /// Volts.
    Volt,
    /// Millivolts.
    Millivolt,
    /// Amperes.
    Ampere,
    /// Milliamperes.
    Milliampere,
    /// Microamperes.
    Microampere,
    /// Unrecognized unit, preserved verbatim.
    Other(String),
}

/// Mapping from a source heading to the stable schema.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ColumnDescriptor {
    /// Original source heading, or an empty string for headerless data.
    pub source_name: String,
    /// Stable canonical name.
    pub canonical_name: String,
    /// Semantic role.
    pub role: ColumnRole,
    /// Parsed unit. Built-in readers use `Some(Unit::Unknown)` when unresolved.
    pub unit: Option<Unit>,
}

/// Stable schema independent of Polars and the source container.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DatasetSchema {
    /// Scientific dataset kind.
    pub kind: DatasetKind,
    /// Columns in their exact source order.
    pub columns: Vec<ColumnDescriptor>,
}

/// A physical numeric column in the canonical consumer model.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Column {
    /// Stable, unique display name.
    pub name: String,
    /// Exact trimmed source header, if the source had one.
    pub original_name: Option<String>,
    /// Scientific role.
    pub role: ColumnRole,
    /// Parsed source unit.
    pub unit: Option<Unit>,
    /// Values in retained source-row order; missing cells are `None`.
    pub values: Vec<Option<f64>>,
}

/// Auditable unit conversion performed by a typed accessor.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UnitConversion {
    /// Scientific role whose values were converted.
    pub role: ColumnRole,
    /// Unit retained from the source.
    pub source_unit: Unit,
    /// Unit returned by the accessor.
    pub target_unit: Unit,
    /// Multiplicative scale applied before the offset.
    pub scale: f64,
    /// Additive offset applied after scaling.
    pub offset: f64,
}

/// Converted values together with the exact deterministic conversion.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConvertedValues {
    pub values: Vec<Option<f64>>,
    pub conversion: UnitConversion,
}
