//! Non-mutating structural and scientific validation.

#[cfg(feature = "scientific-validation")]
use crate::{ColumnRole, DiagnosticCode, FormatId, MetadataValue, Severity};
use crate::{Dataset, Diagnostic, Error, ReadOptions, Result, ValidationLevel};

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
    if dataset.data.height() == 0 {
        return Err(Error::EmptyData);
    }
    for descriptor in &dataset.columns {
        let column = dataset.column_by_role(&descriptor.role)?;
        if column.dtype() != &polars::prelude::DataType::Float64 {
            return Err(Error::InvalidColumnSignature(format!(
                "{} is not Float64",
                descriptor.canonical_name
            )));
        }
        if column.null_count() == column.len() {
            return Err(Error::InvalidColumnSignature(format!(
                "{} is entirely null",
                descriptor.canonical_name
            )));
        }
    }
    Ok(())
}

#[cfg(feature = "scientific-validation")]
fn scientific(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    match dataset.format {
        FormatId::ChiOcpt | FormatId::ChiMultichannelOcpt => {
            validate_ocpt(dataset, options, diagnostics)
        }
        FormatId::ChiEis => validate_eis(dataset, options, diagnostics),
        _ => Ok(()),
    }
}

#[cfg(feature = "scientific-validation")]
fn validate_ocpt(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let time = dataset.column_by_role(&ColumnRole::Time)?.f64()?;
    let mut previous = None;
    let mut last = None;
    for (index, current) in time.iter().enumerate() {
        if let Some(current) = current {
            if !current.is_finite() {
                diagnostics.push(diagnostic(
                    Severity::Error,
                    DiagnosticCode::NonMonotonicTime,
                    "OCPT time is not finite",
                    Some(index + 1),
                    Some("time_s"),
                ));
            }
            if let Some(before) = previous {
                if current == before {
                    diagnostics.push(diagnostic(
                        Severity::Warning,
                        DiagnosticCode::DuplicateTime,
                        "duplicate OCPT timestamp preserved",
                        Some(index + 1),
                        Some("time_s"),
                    ));
                } else if current < before {
                    diagnostics.push(diagnostic(
                        Severity::Warning,
                        DiagnosticCode::NonMonotonicTime,
                        "OCPT time decreases; source order preserved",
                        Some(index + 1),
                        Some("time_s"),
                    ));
                }
            }
            previous = Some(current);
            last = Some(current);
        }
    }
    if let (Some(duration), Some(last)) = (run_time(dataset), last) {
        let relative = (duration - last).abs() / duration.abs().max(f64::EPSILON);
        if relative > options.tolerances.duration_relative {
            diagnostics.push(diagnostic(Severity::Warning, DiagnosticCode::DurationMismatch, &format!("observed final time {last} s differs from declared Run Time {duration} s; neither value was modified"), None, Some("time_s")));
        }
    }
    Ok(())
}

#[cfg(feature = "scientific-validation")]
fn validate_eis(
    dataset: &Dataset,
    options: &ReadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let frequency = dataset.column_by_role(&ColumnRole::Frequency)?.f64()?;
    let real = dataset.column_by_role(&ColumnRole::ImpedanceReal)?.f64()?;
    let imaginary = dataset
        .column_by_role(&ColumnRole::ImpedanceImaginary)?
        .f64()?;
    let magnitude = dataset
        .column_by_role(&ColumnRole::ImpedanceMagnitude)?
        .f64()?;
    let phase = dataset.column_by_role(&ColumnRole::Phase)?.f64()?;
    let frequencies: Vec<Option<f64>> = frequency.iter().collect();
    for (index, value) in frequencies.iter().enumerate() {
        if value.is_some_and(|number| number <= 0.0) {
            diagnostics.push(diagnostic(
                Severity::Error,
                DiagnosticCode::NonPositiveFrequency,
                "EIS frequency must be positive",
                Some(index + 1),
                Some("frequency_hz"),
            ));
        }
    }
    let direction = frequencies
        .windows(2)
        .filter_map(|pair| Some(pair[1]?.total_cmp(&pair[0]?)))
        .find(|order| !order.is_eq());
    if let Some(direction) = direction {
        let monotonic = frequencies.windows(2).all(|pair| match (pair[0], pair[1]) {
            (Some(a), Some(b)) => b.total_cmp(&a) == direction || a == b,
            _ => true,
        });
        if !monotonic {
            diagnostics.push(diagnostic(
                Severity::Warning,
                DiagnosticCode::NonMonotonicFrequency,
                "EIS frequency is not monotonic; source order preserved",
                None,
                Some("frequency_hz"),
            ));
        }
    }
    for index in 0..dataset.data.height() {
        let Some((zr, zi, measured_magnitude, measured_phase)) = real
            .get(index)
            .zip(imaginary.get(index))
            .zip(magnitude.get(index))
            .zip(phase.get(index))
            .map(|(((a, b), c), d)| (a, b, c, d))
        else {
            continue;
        };
        let calculated_magnitude = zr.hypot(zi);
        if (measured_magnitude - calculated_magnitude).abs()
            / calculated_magnitude.max(f64::EPSILON)
            > options.tolerances.eis_magnitude_relative
        {
            diagnostics.push(diagnostic(
                Severity::Warning,
                DiagnosticCode::MagnitudeMismatch,
                "measured EIS magnitude differs from component-derived magnitude",
                Some(index + 1),
                Some("z_magnitude_ohm"),
            ));
        }
        let calculated_phase = zi.atan2(zr).to_degrees();
        if (measured_phase - calculated_phase).abs() > options.tolerances.eis_phase_degrees {
            diagnostics.push(diagnostic(
                Severity::Warning,
                DiagnosticCode::PhaseMismatch,
                "measured EIS phase differs from component-derived phase",
                Some(index + 1),
                Some("phase_deg"),
            ));
        }
    }
    Ok(())
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
fn diagnostic(
    severity: Severity,
    code: DiagnosticCode,
    message: &str,
    row: Option<usize>,
    column: Option<&str>,
) -> Diagnostic {
    Diagnostic {
        severity,
        code,
        message: message.into(),
        row,
        column: column.map(str::to_owned),
    }
}
