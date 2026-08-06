use electrodata_io::{
    ColumnRole, DatasetKind, DiagnosticCode, Error, ErrorCategory, FormatId, InvalidCellPolicy,
    InvalidTimePolicy, RaggedRowPolicy, ReadOptions, ReadProfile, RecoveryAction, SheetSelector,
    TextEncoding, Unit, detect_format, read, read_with_options,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const ALL_FIXTURES: &[&str] = &[
    "regular_two_column.csv",
    "regular_multichannel.csv",
    "regular_headerless.csv",
    "regular_missing_cells.csv",
    "regular_malformed_timestamp.csv",
    "regular_ragged_rows.csv",
    "regular_invalid_numeric.csv",
    "regular_duplicate_timestamps.csv",
    "regular_nonmonotonic_timestamps.csv",
    "generic_text.dat",
    "chi_ocpt.csv",
    "chi_multichannel_ocpt.csv",
    "chi_eis_three_column.csv",
    "chi_eis_four_column.csv",
    "chi_eis_five_column.csv",
    "chi_eis_reordered_columns.csv",
    "timeseries.xlsx",
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rust_electroanalysis_cli")
        .join(name)
}

fn compatibility() -> ReadOptions {
    ReadOptions::new()
        .with_invalid_time_policy(InvalidTimePolicy::SkipRow)
        .with_invalid_cell_policy(InvalidCellPolicy::Null)
        .with_ragged_row_policy(RaggedRowPolicy::PadNulls)
        .with_coordinate_order_policy(electrodata_io::CoordinateOrderPolicy::Preserve)
}

#[test]
fn manifest_drives_every_consumer_fixture() {
    let manifest_text = std::fs::read_to_string(fixture("expected.toml")).unwrap();
    let manifest: toml::Value = manifest_text.parse().unwrap();
    assert_eq!(manifest["schema_version"].as_integer(), Some(1));
    let cases = manifest["fixture"].as_array().unwrap();
    let manifest_files: BTreeSet<_> = cases
        .iter()
        .map(|case| case["file"].as_str().unwrap())
        .collect();
    let required_files: BTreeSet<_> = ALL_FIXTURES.iter().copied().collect();
    assert_eq!(manifest_files, required_files);

    for case in cases {
        let name = case["file"].as_str().unwrap();
        let path = fixture(name);
        assert!(path.is_file(), "missing contract fixture {name}");

        let strict_expectation = case["strict_mode_result"].as_str().unwrap();
        assert_strict_result(name, strict_expectation, read(&path));

        let compatibility_expectation = case["compatibility_mode_result"].as_str().unwrap();
        assert!(
            compatibility_expectation.starts_with("accept"),
            "unsupported compatibility expectation for {name}: {compatibility_expectation}"
        );
        let dataset = read_with_options(&path, &ReadOptions::compatibility()).unwrap();
        assert_eq!(
            kind_name(dataset.kind()),
            case["dataset_kind"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            dataset.canonical_columns().first().unwrap().values.len(),
            case["row_count"].as_integer().unwrap() as usize,
            "{name} row_count"
        );
        assert_eq!(
            dataset.canonical_columns().len(),
            case["column_count"].as_integer().unwrap() as usize,
            "{name} column_count"
        );
        assert_eq!(
            dataset.missing_value_count(),
            case["missing_value_count"].as_integer().unwrap() as usize,
            "{name} missing_value_count"
        );

        let expected_columns = case["columns"].as_array().unwrap();
        assert_eq!(
            dataset.canonical_columns().len(),
            expected_columns.len(),
            "{name}"
        );
        for (position, (column, expected)) in dataset
            .canonical_columns()
            .iter()
            .zip(expected_columns)
            .enumerate()
        {
            let observed_name = column.original_name.as_deref().unwrap_or(&column.name);
            assert_eq!(
                observed_name,
                expected["name"].as_str().unwrap(),
                "{name} column {} name",
                position + 1
            );
            assert_eq!(
                role_name(&column.role),
                expected["role"].as_str().unwrap(),
                "{name} column {} role",
                position + 1
            );
            assert_eq!(
                unit_name(column.unit.as_ref()),
                expected["unit"].as_str().unwrap(),
                "{name} column {} unit",
                position + 1
            );
        }

        let expected_diagnostics = case["expected_diagnostics"].as_array().unwrap();
        assert_eq!(
            dataset.diagnostics.len(),
            expected_diagnostics.len(),
            "{name} diagnostic count: {:#?}",
            dataset.diagnostics
        );
        for (diagnostic, expected) in dataset.diagnostics.iter().zip(expected_diagnostics) {
            assert_eq!(
                diagnostic_code_name(diagnostic.code),
                expected["code"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                severity_name(diagnostic.severity),
                expected["severity"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                diagnostic.row,
                Some(expected["row"].as_integer().unwrap() as usize),
                "{name}"
            );
            let expected_column = expected["column"].as_str().unwrap();
            assert_eq!(
                diagnostic.column.as_deref().unwrap_or("*"),
                expected_column,
                "{name}"
            );
            let expected_column_index = if expected_column == "*" {
                None
            } else if let Some(index) = expected_columns
                .iter()
                .position(|column| column["name"].as_str() == Some(expected_column))
            {
                Some(index + 1)
            } else {
                expected_column
                    .strip_prefix("column_")
                    .and_then(|index| index.parse().ok())
            };
            assert_eq!(diagnostic.column_index, expected_column_index, "{name}");
            assert_eq!(
                recovery_name(diagnostic.recovery),
                expected["action"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                diagnostic.path.as_deref(),
                Some(path.to_str().unwrap()),
                "{name}"
            );
            assert_eq!(
                diagnostic.format_candidate.as_ref(),
                Some(&dataset.format),
                "{name}"
            );
            assert_eq!(diagnostic.profile, ReadProfile::Compatibility, "{name}");
        }
    }
}

fn assert_strict_result(
    name: &str,
    expectation: &str,
    result: electrodata_io::Result<electrodata_io::Dataset>,
) {
    if expectation.starts_with("accept") {
        result.unwrap_or_else(|error| panic!("{name} should be accepted in strict mode: {error}"));
        return;
    }
    let error = result.expect_err(&format!("{name} should be rejected in strict mode"));
    let matches = if expectation.contains("AmbiguousHeaderlessSchema") {
        matches!(&error, Error::AmbiguousHeaderlessSchema { .. })
    } else if expectation.contains("MalformedNumeric") {
        matches!(&error, Error::MalformedNumeric { .. })
    } else if expectation.contains("RaggedRow") {
        matches!(&error, Error::RaggedRow { .. })
    } else if expectation.contains("DuplicateTimestamp") {
        matches!(&error, Error::DuplicateCoordinate { .. })
    } else if expectation.contains("NonMonotonicTimestamp") {
        matches!(&error, Error::NonMonotonicCoordinate { .. })
    } else {
        false
    };
    assert!(matches, "{name}: {expectation}; observed {error:?}");
}

fn kind_name(kind: DatasetKind) -> &'static str {
    match kind {
        DatasetKind::TimeSeries => "TimeSeries",
        DatasetKind::ImpedanceSpectrum => "ImpedanceSpectrum",
        DatasetKind::GenericTable => "GenericTable",
    }
}

fn role_name(role: &ColumnRole) -> String {
    match role {
        ColumnRole::Time => "Time".into(),
        ColumnRole::Frequency => "Frequency".into(),
        ColumnRole::ImpedanceReal => "ImpedanceReal".into(),
        ColumnRole::ImpedanceImaginary => "ImpedanceImaginary".into(),
        ColumnRole::ImpedanceMagnitude => "ImpedanceMagnitude".into(),
        ColumnRole::ImpedancePhase | ColumnRole::Phase => "ImpedancePhase".into(),
        ColumnRole::Potential => "Potential".into(),
        ColumnRole::Current => "Current".into(),
        ColumnRole::MeasurementChannel(channel) | ColumnRole::PotentialChannel(channel) => {
            format!("MeasurementChannel({channel})")
        }
        ColumnRole::Unknown => "Unknown".into(),
        ColumnRole::X => "X".into(),
        ColumnRole::Y => "Y".into(),
        ColumnRole::Custom(value) => format!("Custom({value})"),
        _ => "NonExhaustiveRole".into(),
    }
}

fn unit_name(unit: Option<&Unit>) -> String {
    match unit {
        Some(Unit::Unknown) | None => "Unknown".into(),
        Some(Unit::Second) => "s".into(),
        Some(Unit::Hour) => "h".into(),
        Some(Unit::Day) => "d".into(),
        Some(Unit::Hertz) => "Hz".into(),
        Some(Unit::Ohm) => "ohm".into(),
        Some(Unit::Degree) => "deg".into(),
        Some(Unit::Volt) => "V".into(),
        Some(Unit::Millivolt) => "mV".into(),
        Some(Unit::Ampere) => "A".into(),
        Some(Unit::Milliampere) => "mA".into(),
        Some(Unit::Microampere) => "uA".into(),
        Some(Unit::Other(value)) => value.clone(),
    }
}

fn diagnostic_code_name(code: DiagnosticCode) -> &'static str {
    match code {
        DiagnosticCode::LossyDecoding => "LossyDecoding",
        DiagnosticCode::MissingValue => "MissingValue",
        DiagnosticCode::MalformedTimestamp => "MalformedTimestamp",
        DiagnosticCode::MalformedNumeric => "MalformedNumeric",
        DiagnosticCode::RaggedRow => "RaggedRow",
        DiagnosticCode::RowSkipped => "RowSkipped",
        DiagnosticCode::ValueReplacedWithMissing => "ValueReplacedWithMissing",
        DiagnosticCode::RowPadded => "RowPadded",
        DiagnosticCode::ExtraCellsIgnored => "ExtraCellsIgnored",
        DiagnosticCode::HeaderlessRoleInference => "HeaderlessRoleInference",
        DiagnosticCode::RowClassification => "RowClassification",
        DiagnosticCode::UnknownUnit => "UnknownUnit",
        DiagnosticCode::DuplicateTimestamp | DiagnosticCode::DuplicateTime => "DuplicateTimestamp",
        DiagnosticCode::NonMonotonicTimestamp | DiagnosticCode::NonMonotonicTime => {
            "NonMonotonicTimestamp"
        }
        DiagnosticCode::DurationMismatch => "DurationMismatch",
        DiagnosticCode::NonMonotonicFrequency => "NonMonotonicFrequency",
        DiagnosticCode::NonPositiveFrequency => "NonPositiveFrequency",
        DiagnosticCode::MagnitudeMismatch => "MagnitudeMismatch",
        DiagnosticCode::PhaseMismatch => "PhaseMismatch",
        DiagnosticCode::UnusableFormula => "UnusableFormula",
    }
}

fn severity_name(severity: electrodata_io::Severity) -> &'static str {
    match severity {
        electrodata_io::Severity::Info => "info",
        electrodata_io::Severity::Warning => "warning",
        electrodata_io::Severity::Error => "error",
    }
}

fn recovery_name(recovery: RecoveryAction) -> &'static str {
    match recovery {
        RecoveryAction::None => "None",
        RecoveryAction::RowSkipped => "RowSkipped",
        RecoveryAction::ValueReplacedWithMissing => "ValueReplacedWithMissing",
        RecoveryAction::RowPadded => "RowPadded",
        RecoveryAction::ExtraCellsIgnored => "ExtraCellsIgnored",
        RecoveryAction::Preserved => "Preserved",
        RecoveryAction::RolesInferred => "RolesInferred",
        RecoveryAction::LossyDecoded => "LossyDecoded",
        RecoveryAction::HeaderInterpreted => "HeaderInterpreted",
        RecoveryAction::MetadataPreserved => "MetadataPreserved",
        RecoveryAction::EmptyRowIgnored => "EmptyRowIgnored",
    }
}

#[test]
fn canonical_two_column_and_multichannel_models() {
    let two = read(fixture("regular_two_column.csv")).unwrap();
    assert_eq!(two.kind(), DatasetKind::TimeSeries);
    assert_eq!(two.schema.columns.len(), 2);
    assert_eq!(two.canonical_columns()[0].role, ColumnRole::Time);
    assert_eq!(two.canonical_columns()[0].unit, Some(Unit::Second));
    assert_eq!(two.canonical_columns()[1].role, ColumnRole::Potential);
    assert_eq!(two.canonical_columns()[1].unit, Some(Unit::Volt));
    assert_eq!(two.f64_values(&ColumnRole::Time).unwrap().len(), 3);

    let wide = read(fixture("regular_multichannel.csv")).unwrap();
    assert_eq!(wide.kind(), DatasetKind::TimeSeries);
    assert_eq!(wide.canonical_columns().len(), 5);
    assert_eq!(
        wide.canonical_columns()[1].role,
        ColumnRole::MeasurementChannel(1)
    );
    assert_eq!(
        wide.canonical_columns()[2].role,
        ColumnRole::MeasurementChannel(5)
    );
    assert_eq!(wide.canonical_columns()[3].role, ColumnRole::Current);
    assert_eq!(wide.canonical_columns()[3].unit, Some(Unit::Ampere));
    assert_eq!(wide.canonical_columns()[4].role, ColumnRole::Unknown);
    assert_eq!(
        wide.canonical_columns()[4].unit,
        Some(Unit::Other("C".into()))
    );
    assert_eq!(wide.measurement_channels().len(), 4);
}

#[test]
fn headerless_tables_require_explicit_compatibility_and_retain_all_rows() {
    let path = fixture("regular_headerless.csv");
    assert!(matches!(
        read(&path),
        Err(Error::AmbiguousHeaderlessSchema { .. })
    ));
    let dataset = read_with_options(&path, &compatibility()).unwrap();
    assert_eq!(dataset.kind(), DatasetKind::TimeSeries);
    assert_eq!(dataset.source_row(0), Some(1));
    assert_eq!(dataset.canonical_columns()[0].name, "column_1");
    assert_eq!(dataset.canonical_columns()[1].name, "column_2");
    assert_eq!(dataset.canonical_columns()[1].role, ColumnRole::Unknown);
    assert_eq!(dataset.canonical_columns()[0].unit, Some(Unit::Unknown));
    assert!(dataset.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::HeaderlessRoleInference
            && diagnostic.recovery == RecoveryAction::RolesInferred
    }));

    let wide = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(wide.path(), "0,1,2,3\n1,4,,6\n2,7,8,9\n").unwrap();
    let dataset = read_with_options(wide.path(), &compatibility()).unwrap();
    assert_eq!(dataset.canonical_columns().len(), 4);
    assert_eq!(dataset.canonical_columns()[2].values[1], None);
    assert_eq!(dataset.measurement_channels().len(), 3);
}

#[test]
fn arbitrary_channel_names_units_and_nulls_are_retained() {
    let file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(
        file.path(),
        "Time/s,CH12/mV,Optical/counts\n0,5,100\n1,,101\n2,7,102\n",
    )
    .unwrap();
    let dataset = read(file.path()).unwrap();
    assert_eq!(
        dataset.canonical_columns()[1].role,
        ColumnRole::MeasurementChannel(12)
    );
    assert_eq!(dataset.canonical_columns()[1].unit, Some(Unit::Millivolt));
    assert_eq!(
        dataset.canonical_columns()[2].unit,
        Some(Unit::Other("counts".into()))
    );
    assert_eq!(dataset.canonical_columns()[1].values[1], None);
}

#[test]
fn missing_cells_are_preserved_in_strict_mode_with_locations() {
    let dataset = read(fixture("regular_missing_cells.csv")).unwrap();
    assert_eq!(dataset.missing_value_count(), 2);
    assert_eq!(dataset.f64_values(&ColumnRole::Potential).unwrap()[1], None);
    assert_eq!(dataset.f64_values(&ColumnRole::Current).unwrap()[2], None);
    let locations: Vec<_> = dataset
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::MissingValue)
        .map(|diagnostic| (diagnostic.row, diagnostic.column_index))
        .collect();
    assert_eq!(locations, vec![(Some(3), Some(2)), (Some(4), Some(3))]);
}

#[test]
fn malformed_coordinates_are_structured_errors_or_diagnosed_skips() {
    let path = fixture("regular_malformed_timestamp.csv");
    let error = read(&path).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::MalformedNumeric);
    assert_eq!(error.path(), Some(path.as_path()));
    assert_eq!(error.format_candidate(), Some(&FormatId::RegularXy));
    assert_eq!(error.row(), Some(3));
    assert_eq!(error.column(), Some(1));
    assert!(error.source_error().is_some());
    let context = error.context();
    assert_eq!(context.category, ErrorCategory::MalformedNumeric);
    assert_eq!(context.row, Some(3));

    let dataset = read_with_options(&path, &compatibility()).unwrap();
    assert_eq!(
        dataset.f64_values(&ColumnRole::Time).unwrap(),
        [Some(0.0), Some(2.0)]
    );
    assert_eq!(dataset.source_row(0), Some(2));
    assert_eq!(dataset.source_row(1), Some(4));
    assert!(dataset.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::MalformedTimestamp
            && diagnostic.row == Some(3)
            && diagnostic.column_index == Some(1)
            && diagnostic.recovery == RecoveryAction::RowSkipped
    }));
    assert!(
        dataset
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RowSkipped)
    );
}

#[test]
fn malformed_measurements_are_errors_or_null_recoveries() {
    let path = fixture("regular_invalid_numeric.csv");
    assert!(matches!(
        read(&path),
        Err(Error::MalformedNumeric {
            row: 3,
            column: 2,
            ..
        })
    ));
    let dataset = read_with_options(&path, &compatibility()).unwrap();
    assert_eq!(dataset.f64_values(&ColumnRole::Potential).unwrap()[1], None);
    assert!(dataset.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::ValueReplacedWithMissing
            && diagnostic.row == Some(3)
            && diagnostic.recovery == RecoveryAction::ValueReplacedWithMissing
    }));
}

#[test]
fn ragged_rows_are_errors_or_diagnosed_padding_and_truncation() {
    let path = fixture("regular_ragged_rows.csv");
    assert!(matches!(read(&path), Err(Error::RaggedRow { row: 3, .. })));
    let dataset = read_with_options(&path, &compatibility()).unwrap();
    assert_eq!(dataset.canonical_columns().len(), 3);
    assert_eq!(dataset.canonical_columns()[2].values[1], None);
    for expected in [
        DiagnosticCode::RaggedRow,
        DiagnosticCode::RowPadded,
        DiagnosticCode::ExtraCellsIgnored,
    ] {
        assert!(
            dataset
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == expected),
            "missing {expected:?}"
        );
    }
}

#[test]
fn strict_and_compatibility_coordinate_order_are_explicit() {
    let duplicate = fixture("regular_duplicate_timestamps.csv");
    assert!(matches!(
        read(&duplicate),
        Err(Error::DuplicateCoordinate { row: 4, .. })
    ));
    let dataset = read_with_options(&duplicate, &compatibility()).unwrap();
    assert_eq!(dataset.source_row(2), Some(4));
    assert!(
        dataset
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateTimestamp)
    );

    let nonmonotonic = fixture("regular_nonmonotonic_timestamps.csv");
    assert!(matches!(
        read(&nonmonotonic),
        Err(Error::NonMonotonicCoordinate { row: 4, .. })
    ));
    let dataset = read_with_options(&nonmonotonic, &compatibility()).unwrap();
    assert_eq!(
        dataset.f64_values(&ColumnRole::Time).unwrap(),
        [Some(0.0), Some(1.0), Some(0.5), Some(2.0)]
    );
}

#[test]
fn dat_text_chi_time_series_and_xlsx_share_the_canonical_pipeline() {
    let dat = read(fixture("generic_text.dat")).unwrap();
    assert_eq!(dat.kind(), DatasetKind::TimeSeries);
    assert_eq!(dat.canonical_columns().len(), 3);
    assert_eq!(
        dat.metadata.provenance.container,
        electrodata_io::ContainerFormat::Csv
    );

    let ocpt = read(fixture("chi_ocpt.csv")).unwrap();
    assert_eq!(ocpt.kind(), DatasetKind::TimeSeries);
    let multi = read(fixture("chi_multichannel_ocpt.csv")).unwrap();
    assert_eq!(multi.measurement_channels().len(), 2);

    let xlsx = read(fixture("timeseries.xlsx")).unwrap();
    assert_eq!(xlsx.kind(), DatasetKind::TimeSeries);
    assert_eq!(
        xlsx.metadata.provenance.worksheet.as_deref(),
        Some("measurement")
    );
    assert_eq!(xlsx.canonical_columns().len(), 3);
    assert_eq!(
        xlsx.canonical_columns()[1].role,
        ColumnRole::MeasurementChannel(1)
    );
}

#[test]
fn eis_detection_is_role_based_for_three_four_five_and_reordered_columns() {
    let cases = [
        ("chi_eis_three_column.csv", 3),
        ("chi_eis_four_column.csv", 4),
        ("chi_eis_five_column.csv", 5),
        ("chi_eis_reordered_columns.csv", 5),
    ];
    for (name, width) in cases {
        let dataset = read(fixture(name)).unwrap();
        assert_eq!(dataset.kind(), DatasetKind::ImpedanceSpectrum, "{name}");
        assert_eq!(dataset.canonical_columns().len(), width, "{name}");
        assert_eq!(
            dataset.f64_values(&ColumnRole::Frequency).unwrap()[0],
            Some(1000.0)
        );
        assert_eq!(
            dataset.f64_values(&ColumnRole::ImpedanceReal).unwrap()[0],
            Some(10.0)
        );
        assert_eq!(
            dataset.f64_values(&ColumnRole::ImpedanceImaginary).unwrap()[0],
            Some(-1.0)
        );
    }
}

#[test]
fn eis_typed_view_derives_only_absent_optional_fields() {
    let three = read(fixture("chi_eis_three_column.csv")).unwrap();
    let view = three.eis_view().unwrap();
    assert!(view.measured_magnitude.is_none());
    assert!(view.measured_phase.is_none());
    assert!((view.magnitude_values()[0].unwrap() - 10.049_875_621).abs() < 1e-8);
    assert!((view.phase_values()[0].unwrap() + 5.710_593_137).abs() < 1e-8);
    assert_eq!(three.canonical_columns().len(), 3);

    let four = read(fixture("chi_eis_four_column.csv")).unwrap();
    let view = four.eis_view().unwrap();
    assert!(view.measured_magnitude.is_none());
    assert_eq!(view.phase_values()[0], Some(-5.710_593));

    let five = read(fixture("chi_eis_five_column.csv")).unwrap();
    let view = five.eis_view().unwrap();
    assert_eq!(view.magnitude_values()[0], Some(10.049_876));
    assert_eq!(view.phase_values()[0], Some(-5.710_593));
}

#[test]
fn header_only_eis_is_detected_with_lower_confidence() {
    let file = tempfile::Builder::new().suffix(".dat").tempfile().unwrap();
    std::fs::write(
        file.path(),
        "Z\"/ohm,Freq/Hz,Z'/ohm\n-1,1000,10\n-5,100,15\n-10,10,20\n",
    )
    .unwrap();
    let header_only = detect_format(file.path()).unwrap();
    let preamble = detect_format(fixture("chi_eis_three_column.csv")).unwrap();
    assert_eq!(
        header_only.format,
        electrodata_io::ElectroDataFormat::ChiEis
    );
    assert!(header_only.confidence < preamble.confidence);
    assert_eq!(
        read(file.path()).unwrap().kind(),
        DatasetKind::ImpedanceSpectrum
    );
}

#[test]
fn binary_content_is_rejected_even_with_a_text_extension() {
    let file = tempfile::Builder::new().suffix(".dat").tempfile().unwrap();
    std::fs::write(file.path(), [0_u8, 159, 146, 150, 0, 1]).unwrap();
    let error = read(file.path()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::UnsupportedBinary);
    assert!(matches!(error, Error::UnsupportedBinary { .. }));
}

#[test]
fn typed_time_access_is_unit_checked_and_converted() {
    let hours = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(hours.path(), "Time/h,Potential/V\n0,1\n0.5,2\n1,3\n").unwrap();
    let dataset = read(hours.path()).unwrap();
    assert_eq!(
        dataset.time_seconds().unwrap(),
        [Some(0.0), Some(1_800.0), Some(3_600.0)]
    );
    let view = dataset.time_series_view().unwrap();
    assert_eq!(view.time_conversion.source_unit, Unit::Hour);
    assert_eq!(view.time_conversion.target_unit, Unit::Second);
    assert_eq!(view.time_conversion.scale, 3_600.0);

    let headerless = read_with_options(
        fixture("regular_headerless.csv"),
        &ReadOptions::compatibility(),
    )
    .unwrap();
    assert!(matches!(
        headerless.time_series_view(),
        Err(Error::UnknownUnit {
            role: ColumnRole::Time,
            ..
        })
    ));

    let incompatible = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(incompatible.path(), "Time/V,Potential/V\n0,1\n1,2\n2,3\n").unwrap();
    let dataset = read(incompatible.path()).unwrap();
    assert!(matches!(
        dataset.time_series_view(),
        Err(Error::IncompatibleUnit {
            role: ColumnRole::Time,
            ..
        })
    ));
}

#[test]
fn eis_view_rejects_missing_required_values_and_units() {
    let missing = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(
        missing.path(),
        "Freq/Hz,Z'/ohm,Z\"/ohm\n1000,10,-1\n100,15,\n10,20,-10\n",
    )
    .unwrap();
    let dataset = read(missing.path()).unwrap();
    let error = dataset.eis_view().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::MissingValue);
    assert_eq!(error.row(), Some(3));
    assert_eq!(error.column(), Some(3));

    let wrong_unit = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(
        wrong_unit.path(),
        "Freq/Hz,Z'/V,Z\"/ohm\n1000,10,-1\n100,15,-5\n10,20,-10\n",
    )
    .unwrap();
    let dataset = read(wrong_unit.path()).unwrap();
    assert!(matches!(
        dataset.eis_view(),
        Err(Error::IncompatibleUnit {
            role: ColumnRole::ImpedanceReal,
            ..
        })
    ));
}

#[test]
fn summary_reports_malformed_rows_and_per_column_missing_counts() {
    let dataset = read_with_options(
        fixture("regular_invalid_numeric.csv"),
        &ReadOptions::compatibility(),
    )
    .unwrap();
    let summary = dataset.diagnostic_summary();
    assert_eq!(summary.malformed_cells, 1);
    assert_eq!(summary.malformed_rows, 1);
    assert_eq!(summary.missing_cells, 1);
    assert_eq!(summary.missing_by_column.len(), 2);
    assert_eq!(summary.missing_by_column[0].missing_cells, 0);
    assert_eq!(summary.missing_by_column[1].name, "potential_v");
    assert_eq!(summary.missing_by_column[1].missing_cells, 1);
}

#[test]
fn provenance_retains_detection_policy_columns_and_digest() {
    let dataset = read(fixture("generic_text.dat")).unwrap();
    let provenance = &dataset.metadata.provenance;
    assert_eq!(provenance.delimiter, Some('\t'));
    assert_eq!(provenance.encoding, Some(TextEncoding::Utf8));
    assert_eq!(provenance.detected_format, FormatId::RegularXy);
    assert!(provenance.detection_confidence >= 0.9);
    assert!(!provenance.detection_evidence.is_empty());
    assert_eq!(provenance.read_options.profile, ReadProfile::Strict);
    assert_eq!(provenance.source_columns.len(), 3);
    assert_eq!(provenance.source_columns[2].position, 3);
    assert_eq!(provenance.source_columns[2].source_name, "Current/A");
    assert_eq!(provenance.source_sha256.len(), 64);
    assert!(
        provenance
            .source_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
}

#[test]
fn xlsx_unusable_formula_is_typed_or_diagnosed() {
    use rust_xlsxwriter::{Formula, Workbook};

    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("measurement").unwrap();
    worksheet.write_string(0, 0, "Time/s").unwrap();
    worksheet.write_string(0, 1, "Potential/V").unwrap();
    worksheet.write_number(1, 0, 0.0).unwrap();
    worksheet
        .write_formula(1, 1, Formula::new("=1+1").set_result("not-a-number"))
        .unwrap();
    worksheet.write_number(2, 0, 1.0).unwrap();
    worksheet.write_number(2, 1, 2.0).unwrap();
    worksheet.write_number(3, 0, 2.0).unwrap();
    worksheet.write_number(3, 1, 3.0).unwrap();
    workbook.save(file.path()).unwrap();

    let strict = read(file.path()).unwrap_err();
    assert_eq!(strict.category(), ErrorCategory::Formula);
    assert_eq!(strict.path(), Some(file.path()));
    assert_eq!(strict.worksheet(), Some("measurement"));
    assert_eq!(strict.row(), Some(2));
    assert_eq!(strict.column(), Some(2));

    let dataset = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
    assert_eq!(dataset.f64_values(&ColumnRole::Potential).unwrap()[0], None);
    let diagnostic = dataset
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::UnusableFormula)
        .unwrap();
    assert_eq!(diagnostic.row, Some(2));
    assert_eq!(diagnostic.column_index, Some(2));
    assert_eq!(
        diagnostic.recovery,
        RecoveryAction::ValueReplacedWithMissing
    );
    assert_eq!(dataset.metadata.provenance.formulas.len(), 1);
    assert_eq!(
        dataset.metadata.provenance.formulas[0].cache_status,
        electrodata_io::FormulaCacheStatus::Unusable
    );
}

#[test]
fn worksheet_and_detection_errors_retain_context() {
    use rust_xlsxwriter::Workbook;

    let path = fixture("timeseries.xlsx");
    let error = read_with_options(
        &path,
        &ReadOptions::new().with_sheet(SheetSelector::Name("absent".into())),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Worksheet);
    assert_eq!(error.path(), Some(path.as_path()));
    assert_eq!(error.worksheet(), Some("absent"));
    assert_eq!(error.context().worksheet, Some("absent"));

    let unknown = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(unknown.path(), "not a scientific table\n").unwrap();
    let error = read(unknown.path()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Detection);
    assert_eq!(error.path(), Some(unknown.path()));

    let ambiguous = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    for name in ["first", "second"] {
        let worksheet = workbook.add_worksheet();
        worksheet.set_name(name).unwrap();
        worksheet.write_string(0, 0, "Time/s").unwrap();
        worksheet.write_string(0, 1, "Potential/V").unwrap();
        for row in 1..=3 {
            worksheet.write_number(row, 0, (row - 1) as f64).unwrap();
            worksheet.write_number(row, 1, row as f64).unwrap();
        }
    }
    workbook.save(ambiguous.path()).unwrap();
    let error = detect_format(ambiguous.path()).unwrap_err();
    assert_eq!(error.path(), Some(ambiguous.path()));
    let Error::AmbiguousDetection { candidates, .. } = error else {
        panic!("expected worksheet ambiguity");
    };
    let worksheets: BTreeSet<_> = candidates
        .iter()
        .filter_map(|candidate| candidate.worksheet.as_deref())
        .collect();
    assert_eq!(worksheets, BTreeSet::from(["first", "second"]));
}

#[test]
fn eis_missing_required_role_is_structured() {
    let file = tempfile::Builder::new().suffix(".dat").tempfile().unwrap();
    std::fs::write(
        file.path(),
        "Freq/Hz,Z'/ohm,Z/ohm\n1000,10,10.05\n100,15,15.81\n10,20,22.36\n",
    )
    .unwrap();
    let error = read(file.path()).unwrap_err();
    assert!(matches!(
        error,
        Error::MissingRequiredRole {
            role: ColumnRole::ImpedanceImaginary,
            ..
        }
    ));
}

#[test]
fn individual_recovery_policy_updates_diagnostic_profile() {
    let options = ReadOptions::new().with_invalid_cell_policy(InvalidCellPolicy::Null);
    let dataset = read_with_options(fixture("regular_invalid_numeric.csv"), &options).unwrap();
    assert!(
        dataset
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.profile == ReadProfile::Compatibility)
    );
    assert_eq!(
        dataset.metadata.provenance.read_options.profile,
        ReadProfile::Compatibility
    );
    assert_eq!(
        dataset.metadata.provenance.read_options.invalid_time_policy,
        InvalidTimePolicy::Error
    );
}

#[test]
fn lossy_utf8_has_explicit_recovery_action() {
    let file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(file.path(), b"Time/s,Potenti\xffal/V\n0,1\n1,2\n2,3\n").unwrap();
    let mut options = ReadOptions::new();
    options.allow_lossy_utf8 = true;
    let dataset = read_with_options(file.path(), &options).unwrap();
    let diagnostic = dataset
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::LossyDecoding)
        .unwrap();
    assert_eq!(diagnostic.recovery, RecoveryAction::LossyDecoded);
    assert_eq!(
        dataset.metadata.provenance.encoding,
        Some(TextEncoding::Utf8Lossy)
    );
}

#[test]
fn legacy_xls_is_explicitly_unsupported() {
    let file = tempfile::Builder::new().suffix(".xls").tempfile().unwrap();
    std::fs::write(
        file.path(),
        [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
    )
    .unwrap();
    let error = read(file.path()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::UnsupportedContainer);
    assert_eq!(error.path(), Some(file.path()));
}

#[test]
fn supported_legacy_surfaces_still_compile_and_run() {
    let dataset = electrodata_io::read_regular(fixture("regular_two_column.csv")).unwrap();
    assert_eq!(dataset.data.shape(), (3, 2));
    assert_eq!(ColumnRole::Phase, ColumnRole::ImpedancePhase);
    assert_eq!(
        ColumnRole::PotentialChannel(5),
        ColumnRole::MeasurementChannel(5)
    );

    let channels =
        electrodata_io::read_chi_multichannel_ocpt(fixture("chi_multichannel_ocpt.csv")).unwrap();
    assert_eq!(channels.channel(5).unwrap().len(), 3);
}
