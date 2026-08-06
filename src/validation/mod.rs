//! Non-mutating structural and scientific validation.

use crate::{Dataset, Diagnostic, Error, ReadOptions, Result, ValidationLevel};
#[cfg(feature = "scientific-validation")]
use crate::{DiagnosticCode, MetadataValue, RecoveryAction, Severity};

pub(crate) fn validate(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    if options.validation == ValidationLevel::None {
        return Ok(());
    }
    structural(dataset)?;
    #[cfg(feature = "scientific-validation")]
    if options.validation == ValidationLevel::Scientific {
        scientific(dataset, options, diagnostics)?;
    }
    #[cfg(not(feature = "scientific-validation"))]
    let _ = diagnostics;
    Ok(())
}

fn structural(dataset: &Dataset) -> Result<()> {
    let Some(first) = dataset.canonical_columns().first() else {
        return Err(Error::EmptyData);
    };
    if first.values.is_empty() {
        return Err(Error::EmptyData);
    }
    let height = first.values.len();
    for column in dataset.canonical_columns() {
        if column.values.len() != height {
            return Err(Error::InvalidColumnSignature(format!(
                "{} is not aligned to the dataset row count",
                column.name
            )));
        }
        if required_non_null_role(&column.role) && column.values.iter().all(Option::is_none) {
            return Err(Error::InvalidColumnSignature(format!(
                "{} is entirely null",
                column.name
            )));
        }
    }
    Ok(())
}

fn required_non_null_role(role: &crate::ColumnRole) -> bool {
    matches!(
        role,
        crate::ColumnRole::Time
            | crate::ColumnRole::Frequency
            | crate::ColumnRole::ImpedanceReal
            | crate::ColumnRole::ImpedanceImaginary
    )
}

#[cfg(feature = "scientific-validation")]
fn scientific(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    match dataset.kind() {
        crate::DatasetKind::TimeSeries => validate_duration(dataset, options, diagnostics),
        crate::DatasetKind::ImpedanceSpectrum => validate_eis(dataset, options, diagnostics),
        crate::DatasetKind::GenericTable => Ok(()),
    }
}

#[cfg(feature = "scientific-validation")]
fn validate_duration(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let Some(duration) = run_time(dataset) else {
        return Ok(());
    };
    let Some(last) = dataset
        .f64_values(&crate::ColumnRole::Time)?
        .iter()
        .rev()
        .flatten()
        .next()
        .copied()
    else {
        return Ok(());
    };
    let relative = (duration - last).abs() / duration.abs().max(f64::EPSILON);
    if relative > options.tolerances.duration_relative {
        diagnostics.push(contextual_diagnostic(
            dataset,
            options,
            Severity::Warning,
            DiagnosticCode::DurationMismatch,
            format!(
                "observed final time {last} s differs from declared Run Time {duration} s; neither value was modified"
            ),
            None,
            Some("time_s"),
        ));
    }
    Ok(())
}

#[cfg(feature = "scientific-validation")]
fn validate_eis(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let view = dataset.eis_view()?;
    if let Some(measured) = view.measured_magnitude {
        for (index, (((real, imaginary), measured), source_row)) in view
            .real
            .values
            .iter()
            .zip(&view.imaginary.values)
            .zip(&measured.values)
            .zip((0..).map(|row| dataset.source_row(row)))
            .enumerate()
        {
            let Some((real, imaginary, measured)) = real
                .zip(*imaginary)
                .zip(*measured)
                .map(|((real, imaginary), measured)| (real, imaginary, measured))
            else {
                continue;
            };
            let calculated = real.hypot(imaginary);
            if (measured - calculated).abs() / calculated.max(f64::EPSILON)
                > options.tolerances.eis_magnitude_relative
            {
                diagnostics.push(contextual_diagnostic(
                    dataset,
                    options,
                    Severity::Warning,
                    DiagnosticCode::MagnitudeMismatch,
                    "measured EIS magnitude differs from component-derived magnitude",
                    source_row.or(Some(index + 1)),
                    Some(&measured_name(
                        dataset,
                        crate::ColumnRole::ImpedanceMagnitude,
                    )),
                ));
            }
        }
    }
    if let Some(measured) = view.measured_phase {
        for (index, (((real, imaginary), measured), source_row)) in view
            .real
            .values
            .iter()
            .zip(&view.imaginary.values)
            .zip(&measured.values)
            .zip((0..).map(|row| dataset.source_row(row)))
            .enumerate()
        {
            let Some((real, imaginary, measured)) = real
                .zip(*imaginary)
                .zip(*measured)
                .map(|((real, imaginary), measured)| (real, imaginary, measured))
            else {
                continue;
            };
            let calculated = imaginary.atan2(real).to_degrees();
            if (measured - calculated).abs() > options.tolerances.eis_phase_degrees {
                diagnostics.push(contextual_diagnostic(
                    dataset,
                    options,
                    Severity::Warning,
                    DiagnosticCode::PhaseMismatch,
                    "measured EIS phase differs from component-derived phase",
                    source_row.or(Some(index + 1)),
                    Some(&measured_name(dataset, crate::ColumnRole::ImpedancePhase)),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "scientific-validation")]
fn measured_name(dataset: &Dataset, role: crate::ColumnRole) -> String {
    dataset
        .columns
        .iter()
        .find(|column| column.role == role)
        .map(|column| column.canonical_name.clone())
        .unwrap_or_default()
}

#[cfg(feature = "scientific-validation")]
fn run_time(dataset: &Dataset) -> Option<f64> {
    dataset
        .metadata
        .parameters
        .iter()
        .find(|parameter| parameter.raw_name.eq_ignore_ascii_case("Run Time (sec)"))
        .and_then(|parameter| match parameter.value {
            MetadataValue::Integer(value) => Some(value as f64),
            MetadataValue::Float(value) => Some(value),
            MetadataValue::Text(_) => None,
        })
}

#[cfg(feature = "scientific-validation")]
fn contextual_diagnostic(
    dataset: &Dataset,
    options: &ReadOptions,
    severity: Severity,
    code: DiagnosticCode,
    message: impl Into<String>,
    row: Option<usize>,
    column: Option<&str>,
) -> Diagnostic {
    let mut result = Diagnostic::new(severity, code, message).with_context(
        std::path::Path::new(&dataset.metadata.provenance.path),
        dataset.metadata.provenance.worksheet.as_deref(),
        &dataset.format,
        options.effective_profile(),
    );
    result.row = row;
    result.column = column.map(str::to_owned);
    result.recovery = RecoveryAction::None;
    result
}
