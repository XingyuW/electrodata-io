//! Canonical, Polars-independent dataset access with compatibility escape hatches.

use crate::{
    Column, ColumnDescriptor, ColumnMissingCount, ColumnRole, ConvertedValues, DatasetKind,
    DatasetSchema, Diagnostic, DiagnosticCode, DiagnosticSummary, Error, FormatId, Metadata,
    Result, Unit, UnitConversion, UnitErrorContext,
};
use polars::prelude::{Column as PolarsColumn, DataFrame};
use std::path::Path;

/// A scientific table and all information retained while reading it.
#[derive(Debug, Clone)]
pub struct Dataset {
    /// Compatibility/advanced Polars escape hatch. Ordinary consumers should use typed accessors.
    pub data: DataFrame,
    /// Raw and typed metadata.
    pub metadata: Metadata,
    /// Detected scientific format.
    pub format: FormatId,
    /// Compatibility schema descriptors in exact source order.
    pub columns: Vec<ColumnDescriptor>,
    /// Canonical scientific schema.
    pub schema: DatasetSchema,
    /// Structured diagnostics produced while reading and validating.
    pub diagnostics: Vec<Diagnostic>,
    canonical_columns: Vec<Column>,
    source_rows: Vec<usize>,
    source_data_row_count: usize,
}

/// Borrowed view of an aligned time-series table.
#[derive(Debug, Clone)]
pub struct TimeSeriesView<'a> {
    /// Source coordinate values, retained exactly as decoded from the input.
    ///
    /// This is a physical time coordinate only when [`Self::coordinate_unit`]
    /// establishes a time unit. It is never implicitly interpreted as seconds.
    pub time: &'a Column,
    /// Time values explicitly converted to seconds, when the source unit makes
    /// that conversion justified.
    pub time_seconds: Option<Vec<Option<f64>>>,
    /// Exact conversion applied to `time_seconds`, when available.
    pub time_conversion: Option<UnitConversion>,
    pub measurements: Vec<&'a Column>,
    pub source_rows: &'a [usize],
}

/// Borrowed role-based impedance view.
#[derive(Debug, Clone, Copy)]
pub struct EisView<'a> {
    pub frequency: &'a Column,
    pub real: &'a Column,
    pub imaginary: &'a Column,
    pub measured_magnitude: Option<&'a Column>,
    pub measured_phase: Option<&'a Column>,
}

impl EisView<'_> {
    /// Returns measured magnitude unchanged, or derives it only when the source column is absent.
    pub fn magnitude_values(&self) -> Vec<Option<f64>> {
        if let Some(column) = self.measured_magnitude {
            return column.values.clone();
        }
        self.real
            .values
            .iter()
            .zip(&self.imaginary.values)
            .map(|(real, imaginary)| real.zip(*imaginary).map(|(r, i)| r.hypot(i)))
            .collect()
    }

    /// Returns measured phase unchanged, or derives degrees only when the source column is absent.
    pub fn phase_values(&self) -> Vec<Option<f64>> {
        if let Some(column) = self.measured_phase {
            return column.values.clone();
        }
        self.real
            .values
            .iter()
            .zip(&self.imaginary.values)
            .map(|(real, imaginary)| real.zip(*imaginary).map(|(r, i)| i.atan2(r).to_degrees()))
            .collect()
    }
}

impl Dataset {
    pub(crate) fn from_parts(
        data: DataFrame,
        metadata: Metadata,
        format: FormatId,
        descriptors: Vec<ColumnDescriptor>,
        values: Vec<Vec<Option<f64>>>,
        source_rows: Vec<usize>,
        source_data_row_count: usize,
    ) -> Self {
        let kind = dataset_kind(&descriptors);
        let canonical_columns = descriptors
            .iter()
            .zip(values)
            .map(|(descriptor, values)| Column {
                name: descriptor.canonical_name.clone(),
                original_name: (!descriptor.source_name.is_empty())
                    .then(|| descriptor.source_name.clone()),
                role: descriptor.role.clone(),
                unit: descriptor.unit.clone(),
                values,
            })
            .collect();
        Self {
            data,
            metadata,
            format,
            columns: descriptors.clone(),
            schema: DatasetSchema {
                kind,
                columns: descriptors,
            },
            diagnostics: Vec::new(),
            canonical_columns,
            source_rows,
            source_data_row_count,
        }
    }

    /// Scientific kind independent of instrument/container identity.
    pub fn kind(&self) -> DatasetKind {
        self.schema.kind
    }

    /// Canonical physical columns in exact source order.
    pub fn canonical_columns(&self) -> &[Column] {
        &self.canonical_columns
    }

    /// Returns the unique canonical column with a semantic role.
    pub fn canonical_column(&self, role: &ColumnRole) -> Result<&Column> {
        self.with_provenance_context(unique_by_role(&self.canonical_columns, role))
    }

    /// Null-aware numeric values without exposing Polars.
    pub fn f64_values(&self, role: &ColumnRole) -> Result<&[Option<f64>]> {
        Ok(&self.canonical_column(role)?.values)
    }

    /// Owned compatibility spelling for null-aware role access.
    pub fn optional_f64_by_role(&self, role: &ColumnRole) -> Result<Vec<Option<f64>>> {
        Ok(self.f64_values(role)?.to_vec())
    }

    /// Returns required values, failing with row/source-row context on the first null.
    pub fn required_f64_by_role(&self, role: &ColumnRole) -> Result<Vec<f64>> {
        self.with_provenance_context((|| {
            let column = self.column_index_by_role(role)?;
            self.f64_values(role)?
                .iter()
                .enumerate()
                .map(|(row, value)| {
                    value.ok_or_else(|| Error::MissingValue {
                        path: self.metadata.provenance.path.clone().into(),
                        role: role.clone(),
                        row,
                        source_row: self.source_row(row),
                        column: Some(column + 1),
                    })
                })
                .collect()
        })())
    }

    /// Explicitly converts a numeric role to a requested compatible unit.
    pub fn converted_f64_values(
        &self,
        role: &ColumnRole,
        target_unit: Unit,
    ) -> Result<ConvertedValues> {
        self.with_provenance_context((|| {
            let column_index = self.column_index_by_role(role)?;
            let column = self.canonical_column(role)?;
            let source_unit = column.unit.clone().unwrap_or(Unit::Unknown);
            if source_unit == Unit::Unknown {
                return Err(Error::UnknownUnit {
                    path: self.metadata.provenance.path.clone().into(),
                    role: role.clone(),
                    column: Some(column_index + 1),
                    details: Box::new(UnitErrorContext {
                        source_header: column.original_name.clone(),
                    }),
                });
            }
            let Some((scale, offset)) = conversion(&source_unit, &target_unit) else {
                return Err(Error::IncompatibleUnit {
                    path: self.metadata.provenance.path.clone().into(),
                    role: role.clone(),
                    source_unit,
                    target_unit,
                    column: Some(column_index + 1),
                    details: Box::new(UnitErrorContext {
                        source_header: column.original_name.clone(),
                    }),
                });
            };
            Ok(ConvertedValues {
                values: column
                    .values
                    .iter()
                    .map(|value| value.map(|value| value * scale + offset))
                    .collect(),
                conversion: UnitConversion {
                    role: role.clone(),
                    source_unit,
                    target_unit,
                    scale,
                    offset,
                },
            })
        })())
    }

    /// Returns the time coordinate explicitly converted to seconds.
    pub fn time_seconds(&self) -> Result<Vec<Option<f64>>> {
        Ok(self
            .converted_f64_values(&ColumnRole::Time, Unit::Second)?
            .values)
    }

    /// Returns the original numeric time-coordinate values without unit conversion.
    ///
    /// Headerless inputs may infer the `Time` role while retaining an unknown
    /// source unit. This accessor remains valid in that case.
    pub fn coordinate_values(&self) -> Result<&[Option<f64>]> {
        Ok(&self.canonical_column(&ColumnRole::Time)?.values)
    }

    /// Returns the source unit attached to the time coordinate, if present.
    ///
    /// Built-in readers represent an absent source unit as `Some(Unit::Unknown)`.
    pub fn coordinate_unit(&self) -> Result<Option<&Unit>> {
        Ok(self.canonical_column(&ColumnRole::Time)?.unit.as_ref())
    }

    /// Constructs a typed time-series view.
    pub fn time_series_view(&self) -> Result<TimeSeriesView<'_>> {
        self.with_provenance_context((|| {
            if self.kind() != DatasetKind::TimeSeries {
                return Err(Error::WrongDatasetKind {
                    expected: DatasetKind::TimeSeries,
                    actual: self.kind(),
                });
            }
            let time = self.canonical_column(&ColumnRole::Time)?;
            let normalized =
                matches!(time.unit, Some(ref unit) if conversion(unit, &Unit::Second).is_some())
                    .then(|| self.converted_f64_values(&ColumnRole::Time, Unit::Second))
                    .transpose()?;
            Ok(TimeSeriesView {
                time,
                time_seconds: normalized
                    .as_ref()
                    .map(|converted| converted.values.clone()),
                time_conversion: normalized.map(|converted| converted.conversion),
                measurements: self.measurement_channels(),
                source_rows: &self.source_rows,
            })
        })())
    }

    /// Constructs a role-based EIS view. Optional measured fields are never synthesized in storage.
    pub fn eis_view(&self) -> Result<EisView<'_>> {
        self.with_provenance_context((|| {
            if self.kind() != DatasetKind::ImpedanceSpectrum {
                return Err(Error::InvalidDatasetView {
                    path: self.metadata.provenance.path.clone().into(),
                    requested_view: "EisView".into(),
                    dataset_kind: self.kind(),
                });
            }
            let frequency = self.required_column_in_unit(&ColumnRole::Frequency, Unit::Hertz)?;
            let real = self.required_column_in_unit(&ColumnRole::ImpedanceReal, Unit::Ohm)?;
            let imaginary =
                self.required_column_in_unit(&ColumnRole::ImpedanceImaginary, Unit::Ohm)?;
            let measured_magnitude =
                self.optional_column_in_unit(&ColumnRole::ImpedanceMagnitude, Unit::Ohm)?;
            let measured_phase =
                self.optional_column_in_unit(&ColumnRole::ImpedancePhase, Unit::Degree)?;
            Ok(EisView {
                frequency,
                real,
                imaginary,
                measured_magnitude,
                measured_phase,
            })
        })())
    }

    /// All aligned non-coordinate time-series columns, including unknown roles.
    pub fn measurement_channels(&self) -> Vec<&Column> {
        self.canonical_columns
            .iter()
            .filter(|column| {
                !matches!(
                    column.role,
                    ColumnRole::Time
                        | ColumnRole::Frequency
                        | ColumnRole::ImpedanceReal
                        | ColumnRole::ImpedanceImaginary
                        | ColumnRole::ImpedanceMagnitude
                        | ColumnRole::ImpedancePhase
                        | ColumnRole::Phase
                )
            })
            .collect()
    }

    /// One-based physical source row for a retained zero-based dataset row.
    pub fn source_row(&self, dataset_row: usize) -> Option<usize> {
        self.source_rows.get(dataset_row).copied()
    }

    /// Total number of missing physical cells.
    pub fn missing_value_count(&self) -> usize {
        self.canonical_columns
            .iter()
            .map(|column| column.values.iter().filter(|value| value.is_none()).count())
            .sum()
    }

    /// Missing-value counts for each exact source-position column.
    pub fn missing_value_counts(&self) -> Vec<ColumnMissingCount> {
        self.canonical_columns
            .iter()
            .enumerate()
            .map(|(index, column)| ColumnMissingCount {
                position: index + 1,
                name: column.name.clone(),
                role: column.role.clone(),
                missing_cells: column.values.iter().filter(|value| value.is_none()).count(),
            })
            .collect()
    }

    /// Aggregate counts without rescanning downstream domain objects.
    pub fn diagnostic_summary(&self) -> DiagnosticSummary {
        let malformed_rows = self
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.code,
                    DiagnosticCode::MalformedNumeric
                        | DiagnosticCode::MalformedTimestamp
                        | DiagnosticCode::RaggedRow
                        | DiagnosticCode::UnusableFormula
                )
            })
            .filter_map(|diagnostic| diagnostic.row)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        DiagnosticSummary {
            total_source_rows: self.source_data_row_count,
            retained_rows: self.source_rows.len(),
            skipped_rows: self
                .source_data_row_count
                .saturating_sub(self.source_rows.len()),
            malformed_cells: self
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    matches!(
                        diagnostic.code,
                        DiagnosticCode::MalformedNumeric | DiagnosticCode::MalformedTimestamp
                    )
                })
                .count(),
            malformed_rows,
            missing_cells: self.missing_value_count(),
            missing_by_column: self.missing_value_counts(),
            duplicate_coordinates: self
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    matches!(
                        diagnostic.code,
                        DiagnosticCode::DuplicateTimestamp | DiagnosticCode::DuplicateTime
                    )
                })
                .count(),
            nonmonotonic_transitions: self
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    matches!(
                        diagnostic.code,
                        DiagnosticCode::NonMonotonicTimestamp
                            | DiagnosticCode::NonMonotonicTime
                            | DiagnosticCode::NonMonotonicFrequency
                    )
                })
                .count(),
        }
    }

    /// Compatibility accessor returning a Polars column by semantic role.
    pub fn column_by_role(&self, role: &ColumnRole) -> Result<&PolarsColumn> {
        let indices: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter_map(|(index, item)| (&item.role == role).then_some(index))
            .collect();
        self.with_provenance_context(match indices.as_slice() {
            [] => Err(Error::ColumnRoleNotFound(role.clone())),
            [index] => self
                .data
                .select_at_idx(*index)
                .ok_or_else(|| Error::ColumnRoleNotFound(role.clone())),
            _ => Err(Error::ColumnRoleAmbiguous {
                role: role.clone(),
                matches: indices.len(),
            }),
        })
    }

    /// Compatibility accessor for a numbered measurement/potential channel.
    pub fn channel(&self, channel: u32) -> Result<&PolarsColumn> {
        self.column_by_role(&ColumnRole::MeasurementChannel(channel))
    }

    /// Descriptor at a zero-based source-position index.
    pub fn column_descriptor(&self, index: usize) -> Option<&ColumnDescriptor> {
        self.columns.get(index)
    }

    /// Zero-based source-position indices for all columns with a role.
    pub fn columns_by_role(&self, role: &ColumnRole) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(index, descriptor)| (&descriptor.role == role).then_some(index))
            .collect()
    }

    fn column_index_by_role(&self, role: &ColumnRole) -> Result<usize> {
        let matches = self.columns_by_role(role);
        match matches.as_slice() {
            [] => Err(Error::ColumnRoleNotFound(role.clone())),
            [index] => Ok(*index),
            _ => Err(Error::ColumnRoleAmbiguous {
                role: role.clone(),
                matches: matches.len(),
            }),
        }
    }

    fn required_column_in_unit(&self, role: &ColumnRole, unit: Unit) -> Result<&Column> {
        let column = self
            .optional_column_in_unit(role, unit)?
            .ok_or_else(|| Error::ColumnRoleNotFound(role.clone()))?;
        if let Some(row) = column.values.iter().position(Option::is_none) {
            return Err(Error::MissingValue {
                path: self.metadata.provenance.path.clone().into(),
                role: role.clone(),
                row,
                source_row: self.source_row(row),
                column: Some(self.column_index_by_role(role)? + 1),
            });
        }
        Ok(column)
    }

    fn optional_column_in_unit(&self, role: &ColumnRole, unit: Unit) -> Result<Option<&Column>> {
        let matches: Vec<(usize, &Column)> = self
            .canonical_columns
            .iter()
            .enumerate()
            .filter(|(_, column)| &column.role == role)
            .collect();
        let (index, column) = match matches.as_slice() {
            [] => return Ok(None),
            [match_] => *match_,
            _ => {
                return Err(Error::ColumnRoleAmbiguous {
                    role: role.clone(),
                    matches: matches.len(),
                });
            }
        };
        let source_unit = column.unit.clone().unwrap_or(Unit::Unknown);
        if source_unit == Unit::Unknown {
            return Err(Error::UnknownUnit {
                path: self.metadata.provenance.path.clone().into(),
                role: role.clone(),
                column: Some(index + 1),
                details: Box::new(UnitErrorContext {
                    source_header: column.original_name.clone(),
                }),
            });
        }
        if conversion(&source_unit, &unit) != Some((1.0, 0.0)) {
            return Err(Error::IncompatibleUnit {
                path: self.metadata.provenance.path.clone().into(),
                role: role.clone(),
                source_unit,
                target_unit: unit,
                column: Some(index + 1),
                details: Box::new(UnitErrorContext {
                    source_header: column.original_name.clone(),
                }),
            });
        }
        Ok(Some(column))
    }

    fn with_provenance_context<T>(&self, result: Result<T>) -> Result<T> {
        if self.metadata.provenance.worksheet.is_none() {
            return result;
        }
        result.map_err(|error| {
            error.with_read_context_at(
                Path::new(&self.metadata.provenance.path),
                self.metadata.provenance.worksheet.as_deref(),
                self.metadata.provenance.worksheet_index,
                Some(self.format.clone()),
            )
        })
    }
}

impl TimeSeriesView<'_> {
    /// Returns source coordinate values without physical-unit normalization.
    pub fn coordinate_values(&self) -> &[Option<f64>] {
        &self.time.values
    }

    /// Returns the source unit of the coordinate, including `Unit::Unknown`.
    pub fn coordinate_unit(&self) -> Option<&Unit> {
        self.time.unit.as_ref()
    }

    /// Returns normalized seconds only when the source coordinate has a
    /// recognized time unit. Use [`Dataset::time_seconds`] when an unavailable
    /// normalization must produce a structured error.
    pub fn normalized_seconds(&self) -> Option<&[Option<f64>]> {
        self.time_seconds.as_deref()
    }
}

fn unique_by_role<'a>(columns: &'a [Column], role: &ColumnRole) -> Result<&'a Column> {
    let matches: Vec<&Column> = columns
        .iter()
        .filter(|column| &column.role == role)
        .collect();
    match matches.as_slice() {
        [] => Err(Error::ColumnRoleNotFound(role.clone())),
        [column] => Ok(column),
        _ => Err(Error::ColumnRoleAmbiguous {
            role: role.clone(),
            matches: matches.len(),
        }),
    }
}

fn dataset_kind(columns: &[ColumnDescriptor]) -> DatasetKind {
    let has = |role: &ColumnRole| columns.iter().any(|column| &column.role == role);
    if has(&ColumnRole::Frequency)
        && has(&ColumnRole::ImpedanceReal)
        && has(&ColumnRole::ImpedanceImaginary)
    {
        DatasetKind::ImpedanceSpectrum
    } else if has(&ColumnRole::Time) && columns.len() > 1 {
        DatasetKind::TimeSeries
    } else {
        DatasetKind::GenericTable
    }
}

fn conversion(source: &Unit, target: &Unit) -> Option<(f64, f64)> {
    if source == target {
        return Some((1.0, 0.0));
    }
    match (source, target) {
        (Unit::Hour, Unit::Second) => Some((3_600.0, 0.0)),
        (Unit::Day, Unit::Second) => Some((86_400.0, 0.0)),
        (Unit::Second, Unit::Hour) => Some((1.0 / 3_600.0, 0.0)),
        (Unit::Second, Unit::Day) => Some((1.0 / 86_400.0, 0.0)),
        (Unit::Millivolt, Unit::Volt) => Some((0.001, 0.0)),
        (Unit::Volt, Unit::Millivolt) => Some((1_000.0, 0.0)),
        (Unit::Milliampere, Unit::Ampere) => Some((0.001, 0.0)),
        (Unit::Microampere, Unit::Ampere) => Some((0.000_001, 0.0)),
        (Unit::Ampere, Unit::Milliampere) => Some((1_000.0, 0.0)),
        (Unit::Ampere, Unit::Microampere) => Some((1_000_000.0, 0.0)),
        _ => None,
    }
}
