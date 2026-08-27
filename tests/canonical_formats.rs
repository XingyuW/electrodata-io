use std::path::{Path, PathBuf};

use electrodata_io::{
    ColumnRole, DiagnosticCode, ElectroDataFormat, FormatId, ReadOptions, Unit, detect_format,
    read, read_with_options,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("rust_electroanalysis_cli")
        .join(name)
}

#[test]
fn canonical_regular_fixture_executes_production_reader() {
    let path = fixture("regular_two_column.csv");
    assert!(
        path.is_file(),
        "required committed fixture is missing: {path:?}"
    );

    let detection = detect_format(&path).expect("regular fixture should be detected");
    assert_eq!(detection.format, ElectroDataFormat::Regular);
    assert_eq!(detection.header_row, Some(0));
    assert_eq!(detection.data_start_row, 1);

    let dataset = read(&path).expect("regular fixture should be readable");
    assert_eq!(dataset.format, FormatId::RegularXy);
    assert_eq!(dataset.data.shape(), (3, 2));
    assert_eq!(dataset.columns[0].role, ColumnRole::Time);
    assert_eq!(dataset.columns[0].unit, Some(Unit::Second));
    assert_eq!(dataset.columns[1].role, ColumnRole::Potential);
    assert_eq!(
        dataset.data.column("time_s").unwrap().f64().unwrap().get(1),
        Some(1.0)
    );
    assert_eq!(
        dataset
            .data
            .column("potential_v")
            .unwrap()
            .f64()
            .unwrap()
            .get(2),
        Some(0.103)
    );

    let duplicate_path = fixture("regular_duplicate_timestamps.csv");
    assert!(
        duplicate_path.is_file(),
        "required committed fixture is missing: {duplicate_path:?}"
    );
    let duplicate = read_with_options(&duplicate_path, &ReadOptions::compatibility())
        .expect("duplicate-timestamp fixture should be readable");
    assert_eq!(duplicate.data.shape(), (4, 2));
    assert_eq!(
        duplicate
            .f64_values(&ColumnRole::Time)
            .expect("time values should be available"),
        &[Some(0.0), Some(1.0), Some(1.0), Some(2.0)]
    );
    assert!(duplicate.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            DiagnosticCode::DuplicateTime | DiagnosticCode::DuplicateTimestamp
        )
    }));
}

#[test]
fn canonical_chi_ocpt_fixture_executes_production_reader() {
    let path = fixture("chi_ocpt.csv");
    assert!(
        path.is_file(),
        "required committed fixture is missing: {path:?}"
    );

    let detection = detect_format(&path).expect("CHI OCPT fixture should be detected");
    assert_eq!(detection.format, ElectroDataFormat::ChiOcpt);
    assert_eq!(detection.header_row, Some(7));
    assert_eq!(detection.data_start_row, 9);

    let dataset = read(&path).expect("CHI OCPT fixture should be readable");
    assert_eq!(dataset.format, FormatId::ChiOcpt);
    assert_eq!(dataset.data.shape(), (3, 2));
    assert_eq!(dataset.columns[0].role, ColumnRole::Time);
    assert_eq!(dataset.columns[0].unit, Some(Unit::Second));
    assert_eq!(dataset.columns[1].role, ColumnRole::Potential);
    assert_eq!(dataset.metadata.raw_rows.len(), 7);
    assert_eq!(
        dataset.metadata.acquisition.technique.as_deref(),
        Some("Open Circuit Potential - Time")
    );
    assert_eq!(
        dataset.metadata.acquisition.instrument_model.as_deref(),
        Some("CHI760F")
    );
    assert!(
        dataset
            .metadata
            .parameters
            .iter()
            .any(|parameter| parameter.raw_name == "Run Time (sec)" && parameter.raw_value == "2")
    );
}

#[test]
fn canonical_chi_multichannel_fixture_executes_production_reader() {
    let path = fixture("chi_multichannel_ocpt.csv");
    assert!(
        path.is_file(),
        "required committed fixture is missing: {path:?}"
    );

    let detection = detect_format(&path).expect("CHI multichannel fixture should be detected");
    assert_eq!(detection.format, ElectroDataFormat::ChiMultichannelOcpt);
    assert_eq!(detection.header_row, Some(7));
    assert_eq!(detection.data_start_row, 9);

    let dataset = read(&path).expect("CHI multichannel fixture should be readable");
    assert_eq!(dataset.format, FormatId::ChiMultichannelOcpt);
    assert_eq!(dataset.data.shape(), (3, 3));
    assert_eq!(
        dataset
            .columns
            .iter()
            .map(|column| column.role.clone())
            .collect::<Vec<_>>(),
        vec![
            ColumnRole::Time,
            ColumnRole::MeasurementChannel(1),
            ColumnRole::MeasurementChannel(5)
        ]
    );
    assert_eq!(dataset.channel(1).unwrap().len(), 3);
    assert_eq!(dataset.channel(5).unwrap().len(), 3);
    assert_eq!(
        dataset
            .f64_values(&ColumnRole::MeasurementChannel(5))
            .expect("channel 5 values should be available"),
        &[Some(-0.1051), Some(-0.1040), Some(-0.1028)]
    );
}

#[test]
fn canonical_chi_eis_fixture_executes_production_reader() {
    let path = fixture("chi_eis_three_column.csv");
    assert!(
        path.is_file(),
        "required committed fixture is missing: {path:?}"
    );

    let detection = detect_format(&path).expect("CHI EIS fixture should be detected");
    assert_eq!(detection.format, ElectroDataFormat::ChiEis);
    assert_eq!(detection.header_row, Some(6));
    assert_eq!(detection.data_start_row, 8);

    let dataset = read(&path).expect("CHI EIS fixture should be readable");
    assert_eq!(dataset.format, FormatId::ChiEis);
    let view = dataset.eis_view().expect("EIS view should be available");
    assert_eq!(
        view.frequency.values,
        vec![Some(1000.0), Some(100.0), Some(10.0)]
    );
    assert!(
        view.imaginary
            .values
            .iter()
            .all(|value| value.is_some_and(|value| value < 0.0))
    );
    assert!(view.measured_magnitude.is_none());
    assert!(view.measured_phase.is_none());
    assert!((view.magnitude_values()[0].unwrap() - 10.049875621).abs() < 1e-9);
    assert!(view.phase_values()[0].unwrap() < 0.0);
    assert!(
        view.frequency
            .values
            .windows(2)
            .all(|values| values[0] > values[1])
    );
    assert_eq!(dataset.metadata.raw_rows.len(), 6);
}
