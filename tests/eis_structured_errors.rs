use electrodata_io::{
    ColumnRole, DatasetKind, DiagnosticCode, Error, FormatId, ReadOptions, Severity, Unit, read,
    read_chi_eis, read_with_options,
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
    expected_source_headers: &[&str],
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
            assert_eq!(diagnostic.source_headers, expected_source_headers);
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
        &["Z'/ohm", "Z\"/ohm", "Z/ohm"],
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
        &["Freq/Hz", "Z\"/ohm", "Z/ohm"],
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
            &["Freq/Hz", "Z'/ohm"],
        );
    }
}

#[test]
fn forced_eis_public_api_validates_zero_through_five_recognized_roles() {
    let cases = [
        (
            "zero roles",
            "Potential/V,Current/A",
            "1,2\n2,3\n",
            Some((ColumnRole::Frequency, vec![])),
        ),
        (
            "frequency only",
            "Freq/Hz,Value",
            "1,100\n10,80\n",
            Some((ColumnRole::ImpedanceReal, vec![ColumnRole::Frequency])),
        ),
        (
            "real only",
            "Z'/ohm,Value",
            "100,1\n80,2\n",
            Some((ColumnRole::Frequency, vec![ColumnRole::ImpedanceReal])),
        ),
        (
            "imaginary only",
            "Z''/ohm,Value",
            "-20,1\n-10,2\n",
            Some((ColumnRole::Frequency, vec![ColumnRole::ImpedanceImaginary])),
        ),
        (
            "frequency and real",
            "Freq/Hz,Z'/ohm",
            "1,100\n10,80\n",
            Some((
                ColumnRole::ImpedanceImaginary,
                vec![ColumnRole::Frequency, ColumnRole::ImpedanceReal],
            )),
        ),
        (
            "frequency and imaginary",
            "Freq/Hz,Z''/ohm",
            "1,-20\n10,-10\n",
            Some((
                ColumnRole::ImpedanceReal,
                vec![ColumnRole::Frequency, ColumnRole::ImpedanceImaginary],
            )),
        ),
        (
            "real and imaginary",
            "Z'/ohm,Z''/ohm",
            "100,-20\n80,-10\n",
            Some((
                ColumnRole::Frequency,
                vec![ColumnRole::ImpedanceReal, ColumnRole::ImpedanceImaginary],
            )),
        ),
        (
            "three required roles",
            "Freq/Hz,Z'/ohm,Z''/ohm",
            "1,100,-20\n10,80,-10\n",
            None,
        ),
        (
            "required roles and magnitude",
            "Freq/Hz,Z'/ohm,Z''/ohm,Z/ohm",
            "1,100,-20,102\n10,80,-10,81\n",
            None,
        ),
        (
            "required roles and phase",
            "Freq/Hz,Z'/ohm,Z''/ohm,Phase/deg",
            "1,100,-20,-11\n10,80,-10,-7\n",
            None,
        ),
        (
            "all five roles",
            "Freq/Hz,Z'/ohm,Z''/ohm,Z/ohm,Phase/deg",
            "1,100,-20,102,-11\n10,80,-10,81,-7\n",
            None,
        ),
        (
            "reordered complete roles",
            "Phase/deg,Z''/ohm,Freq/Hz,Z/ohm,Z'/ohm",
            "-11,-20,1,102,100\n-7,-10,10,81,80\n",
            None,
        ),
    ];

    for (name, header, rows, expected_error) in cases {
        let file = temporary_csv(&format!("{header}\n{rows}"));
        let result = read_chi_eis(file.path());
        if let Some((missing_role, detected_roles)) = expected_error {
            let source_headers: Vec<_> = header.split(',').collect();
            assert_missing_required_role(
                result.unwrap_err(),
                file.path(),
                missing_role,
                &detected_roles,
                &source_headers,
            );
        } else {
            let dataset = result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(dataset.kind(), DatasetKind::ImpedanceSpectrum, "{name}");
            assert_eq!(
                dataset
                    .canonical_columns()
                    .iter()
                    .map(|column| column.original_name.as_deref().unwrap())
                    .collect::<Vec<_>>(),
                header.split(',').collect::<Vec<_>>(),
                "{name}"
            );
            assert!(
                dataset
                    .canonical_columns()
                    .iter()
                    .all(|column| column.role != ColumnRole::Unknown),
                "{name}"
            );
            let view = dataset
                .eis_view()
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(view.frequency.values.len(), 2, "{name}");
            assert_eq!(view.real.values.len(), 2, "{name}");
            assert_eq!(view.imaginary.values.len(), 2, "{name}");
            assert_eq!(
                view.measured_magnitude.is_some(),
                header.split(',').any(|source| source == "Z/ohm"),
                "{name}"
            );
            assert_eq!(
                view.measured_phase.is_some(),
                header.split(',').any(|source| source == "Phase/deg"),
                "{name}"
            );
        }
    }
}

#[test]
fn incomplete_eis_headers_remain_non_eis_automatically() {
    let cases = [
        "Potential/V,Current/A\n1,2\n2,3\n",
        "Freq/Hz,Value\n1,100\n10,80\n",
        "Z'/ohm,Value\n100,1\n80,2\n",
        "Z''/ohm,Value\n-20,1\n-10,2\n",
        "Freq/Hz,Z'/ohm\n1,100\n10,80\n",
        "Freq/Hz,Z''/ohm\n1,-20\n10,-10\n",
        "Z'/ohm,Z''/ohm\n100,-20\n80,-10\n",
    ];

    for contents in cases {
        let file = temporary_csv(contents);
        assert_ne!(
            electrodata_io::detect_format(file.path()).unwrap().format,
            electrodata_io::ElectroDataFormat::ChiEis
        );
        assert_ne!(
            read(file.path()).unwrap().kind(),
            DatasetKind::ImpedanceSpectrum
        );
    }
}

#[test]
fn complete_header_only_eis_remains_valid_without_a_chi_preamble() {
    let file = temporary_csv("Freq/Hz,Z'/ohm,Z\"/ohm\n1,100,-20\n10,80,-10\n");

    let dataset = read(file.path()).unwrap();
    assert_eq!(dataset.kind(), DatasetKind::ImpedanceSpectrum);
    assert_eq!(
        dataset.eis_view().unwrap().frequency.values,
        [Some(1.0), Some(10.0)]
    );
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
