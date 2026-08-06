use electrodata_io::{
    ColumnRole, DatasetKind, DiagnosticCode, Error, FormatId, ReadOptions, Severity, Unit, read,
    read_with_options,
};
use std::error::Error as StdError;
use std::path::Path;

fn temporary_csv(contents: &str) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(file.path(), contents).unwrap();
    file
}

fn assert_missing_required_role(
    error: Error,
    path: &Path,
    expected_role: ColumnRole,
    expected_detected_roles: &[ColumnRole],
) {
    assert!(StdError::source(&error).is_none());
    match error {
        Error::MissingRequiredRole {
            path: error_path,
            format_candidate,
            role,
            detected_roles,
            diagnostics,
        } => {
            assert_eq!(error_path, path);
            assert_eq!(format_candidate, FormatId::ChiEis);
            assert_eq!(role, expected_role);
            assert_eq!(detected_roles, expected_detected_roles);
            assert_eq!(diagnostics.len(), 1);
            let diagnostic = &diagnostics[0];
            assert_eq!(diagnostic.code, DiagnosticCode::EisSchema);
            assert_eq!(diagnostic.severity, Severity::Error);
            assert_eq!(diagnostic.path.as_deref(), path.to_str());
            assert_eq!(diagnostic.format_candidate, Some(FormatId::ChiEis));
            assert_eq!(diagnostic.recovery, electrodata_io::RecoveryAction::None);
            assert_eq!(
                diagnostic.resolution.as_deref(),
                Some("rejected: missing required EIS role")
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn read_with_options_preserves_missing_frequency_fields() {
    let file = temporary_csv("Z'/ohm,Z\"/ohm,Z/ohm\n10,-1,10.05\n15,-5,15.81\n20,-10,22.36\n");

    let error = read_with_options(file.path(), &ReadOptions::new()).unwrap_err();
    assert_missing_required_role(
        error,
        file.path(),
        ColumnRole::Frequency,
        &[
            ColumnRole::ImpedanceReal,
            ColumnRole::ImpedanceImaginary,
            ColumnRole::ImpedanceMagnitude,
        ],
    );
}

#[test]
fn read_preserves_missing_real_impedance_fields() {
    let file = temporary_csv("Freq/Hz,Z\"/ohm,Z/ohm\n1000,-1,10.05\n100,-5,15.81\n10,-10,22.36\n");

    let error = read(file.path()).unwrap_err();
    assert_missing_required_role(
        error,
        file.path(),
        ColumnRole::ImpedanceReal,
        &[
            ColumnRole::Frequency,
            ColumnRole::ImpedanceImaginary,
            ColumnRole::ImpedanceMagnitude,
        ],
    );
}

#[test]
fn public_reads_preserve_two_column_missing_imaginary_impedance_fields() {
    let file = temporary_csv("A.C. Impedance\n\nFreq/Hz,Z'/ohm\n1000,10\n100,15\n10,20\n");

    for error in [
        read(file.path()).unwrap_err(),
        read_with_options(file.path(), &ReadOptions::new()).unwrap_err(),
        read_with_options(
            file.path(),
            &ReadOptions::new().with_format(FormatId::ChiEis),
        )
        .unwrap_err(),
    ] {
        assert_missing_required_role(
            error,
            file.path(),
            ColumnRole::ImpedanceImaginary,
            &[ColumnRole::Frequency, ColumnRole::ImpedanceReal],
        );
    }
}

#[test]
fn read_preserves_duplicate_frequency_conflict_fields() {
    let file = temporary_csv("Freq/Hz,Frequency/Hz,Z'/ohm\n1000,1000,10\n100,100,15\n10,10,20\n");

    let error = read(file.path()).unwrap_err();
    assert!(StdError::source(&error).is_none());
    match error {
        Error::EisSchemaConflict { details } => {
            assert_eq!(details.path, file.path());
            assert_eq!(details.format_candidate, FormatId::ChiEis);
            assert_eq!((details.row, details.column), (1, 2));
            assert_eq!(details.header, "Frequency/Hz");
            assert_eq!(details.candidate_roles, [ColumnRole::Frequency]);
            assert_eq!(details.conflicting_role, Some(ColumnRole::Frequency));
            assert_eq!(details.diagnostics.len(), 1);
            let diagnostic = &details.diagnostics[0];
            assert_eq!(diagnostic.code, DiagnosticCode::EisSchema);
            assert_eq!(diagnostic.column.as_deref(), Some("Frequency/Hz"));
            assert_eq!(diagnostic.column_index, Some(2));
            assert_eq!(diagnostic.candidate_roles, [ColumnRole::Frequency]);
            assert_eq!(
                diagnostic.resolution.as_deref(),
                Some("rejected: duplicate EIS role")
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn read_with_options_preserves_ambiguous_header_fields() {
    let file =
        temporary_csv("Freq/Hz,\"Z' + Z\"\"\",Z/ohm\n1000,10,10.05\n100,15,15.81\n10,20,22.36\n");

    let error = read_with_options(file.path(), &ReadOptions::new()).unwrap_err();
    assert!(StdError::source(&error).is_none());
    match error {
        Error::EisSchemaConflict { details } => {
            assert_eq!(details.path, file.path());
            assert_eq!(details.format_candidate, FormatId::ChiEis);
            assert_eq!((details.row, details.column), (1, 2));
            assert_eq!(details.header, "Z' + Z\"");
            assert_eq!(
                details.candidate_roles,
                [ColumnRole::ImpedanceReal, ColumnRole::ImpedanceImaginary]
            );
            assert_eq!(details.conflicting_role, None);
            assert_eq!(details.diagnostics.len(), 1);
            let diagnostic = &details.diagnostics[0];
            assert_eq!(diagnostic.code, DiagnosticCode::EisSchema);
            assert_eq!(diagnostic.column.as_deref(), Some("Z' + Z\""));
            assert_eq!(diagnostic.column_index, Some(2));
            assert_eq!(diagnostic.candidate_roles, details.candidate_roles);
            assert_eq!(
                diagnostic.resolution.as_deref(),
                Some("rejected: ambiguous EIS header")
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn eis_view_preserves_incompatible_unit_fields() {
    let file = temporary_csv("Freq/Hz,Z'/V,Z\"/ohm\n1000,10,-1\n100,15,-5\n10,20,-10\n");
    let dataset = read(file.path()).unwrap();

    let error = dataset.eis_view().unwrap_err();
    assert!(StdError::source(&error).is_none());
    match error {
        Error::IncompatibleUnit {
            path,
            role,
            source_unit,
            target_unit,
            column,
            details,
        } => {
            assert_eq!(path, file.path());
            assert_eq!(role, ColumnRole::ImpedanceReal);
            assert_eq!(source_unit, Unit::Volt);
            assert_eq!(target_unit, Unit::Ohm);
            assert_eq!(column, Some(2));
            assert_eq!(details.source_header.as_deref(), Some("Z'/V"));
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn eis_view_preserves_regular_dataset_view_fields() {
    let file = temporary_csv("Time/s,Potential/V\n0,1\n1,2\n2,3\n");
    let dataset = read(file.path()).unwrap();

    let error = dataset.eis_view().unwrap_err();
    assert!(StdError::source(&error).is_none());
    match error {
        Error::InvalidDatasetView {
            path,
            requested_view,
            dataset_kind,
        } => {
            assert_eq!(path, file.path());
            assert_eq!(requested_view, "EisView");
            assert_eq!(dataset_kind, DatasetKind::TimeSeries);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn public_strict_numeric_error_retains_parse_source() {
    let file =
        temporary_csv("Freq/Hz,Z'/ohm,Z\"/ohm\n1000,10,-1\n100,not-a-number,-5\n10,20,-10\n");

    let error = read_with_options(file.path(), &ReadOptions::new()).unwrap_err();
    match &error {
        Error::MalformedNumeric {
            path,
            format_candidate,
            row,
            column,
            value,
            source,
            ..
        } => {
            assert_eq!(path, file.path());
            assert_eq!(format_candidate, &Some(FormatId::ChiEis));
            assert_eq!((*row, *column), (3, 2));
            assert_eq!(value, "not-a-number");
            assert!(source.is_some());
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
    let source = StdError::source(&error).expect("parse source must remain chained");
    assert!(source.downcast_ref::<std::num::ParseFloatError>().is_some());
}
