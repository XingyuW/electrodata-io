use electrodata_io::{
    ColumnRole, DiagnosticCode, FormatId, ReadOptions, ValidationLevel, read_with_options,
};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(name)
}

fn value(dataset: &electrodata_io::Dataset, column: &str, row: usize) -> f64 {
    dataset
        .data
        .column(column)
        .unwrap()
        .f64()
        .unwrap()
        .get(row)
        .unwrap()
}

#[test]
fn supplied_regular_file_is_preserved_as_present() {
    if !fixture("regular_file.csv").exists() {
        return;
    }
    let dataset = electrodata_io::read_regular(fixture("regular_file.csv")).unwrap();
    assert_eq!(dataset.format, FormatId::RegularXy);
    // The checked-in fixture has one header plus 459,884 data rows; its final
    // physical line has no trailing newline.
    assert_eq!(dataset.data.shape(), (459_884, 2));
    assert_eq!(dataset.data.get_column_names(), ["x", "y"]);
    assert_eq!(value(&dataset, "x", 0), 0.0);
    assert!((value(&dataset, "y", 0) - 123.6).abs() < 1e-12);
    assert!((value(&dataset, "x", dataset.data.height() - 1) - 53.23).abs() < 1e-12);
    assert!((value(&dataset, "y", dataset.data.height() - 1) - 167.05).abs() < 1e-12);
    assert_eq!(dataset.columns[0].source_name, "Time / S");
}

#[test]
fn supplied_ocpt_metadata_and_rows_are_preserved() {
    if !fixture("OCPT.csv").exists() {
        return;
    }
    let options = ReadOptions::compatibility().with_validation(ValidationLevel::Scientific);
    let dataset = read_with_options(fixture("OCPT.csv"), &options).unwrap();
    assert_eq!(dataset.format, FormatId::ChiOcpt);
    assert_eq!(dataset.data.shape(), (1_205, 2));
    assert_eq!(dataset.data.get_column_names(), ["time_s", "potential_v"]);
    assert_eq!(dataset.columns[0].role, ColumnRole::Time);
    assert_eq!(dataset.columns[1].role, ColumnRole::Potential);
    assert_eq!(dataset.columns[0].source_name, "Time/sec");
    assert_eq!(
        dataset.metadata.acquisition.instrument_model.as_deref(),
        Some("CHI760F")
    );
    assert_eq!(
        dataset.metadata.acquisition.technique.as_deref(),
        Some("Open Circuit Potential - Time")
    );
    assert_eq!(
        dataset
            .metadata
            .acquisition
            .recorded_at
            .unwrap()
            .to_string(),
        "2026-03-06 13:55:19"
    );
    let interval = dataset
        .metadata
        .parameters
        .iter()
        .find(|p| p.raw_name == "Sample Interval (V)")
        .unwrap();
    assert_eq!(interval.unit.as_deref(), Some("V"));
    assert_eq!(interval.raw_value, "0.1");
    assert_eq!(value(&dataset, "time_s", 0), 0.1);
    assert!((value(&dataset, "potential_v", 0) - 0.2482).abs() < 1e-12);
    assert_eq!(value(&dataset, "time_s", 1_204), 120.5);
    assert!((value(&dataset, "potential_v", 1_204) - 0.1281).abs() < 1e-12);
    assert!(
        dataset
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::DurationMismatch)
    );
}

#[test]
fn supplied_multichannel_duplicate_and_order_are_preserved() {
    if !fixture("Multichannel_OCPT.csv").exists() {
        return;
    }
    let options = ReadOptions::compatibility().with_validation(ValidationLevel::Scientific);
    let dataset = read_with_options(fixture("Multichannel_OCPT.csv"), &options).unwrap();
    assert_eq!(dataset.format, FormatId::ChiMultichannelOcpt);
    assert_eq!(dataset.data.shape(), (10_001, 3));
    assert_eq!(
        dataset.data.get_column_names(),
        ["time_s", "ch1_v", "ch5_v"]
    );
    assert_eq!(dataset.columns[1].role, ColumnRole::PotentialChannel(1));
    assert_eq!(dataset.columns[2].role, ColumnRole::PotentialChannel(5));
    assert_eq!(dataset.channel(5).unwrap().name().as_str(), "ch5_v");
    let times = dataset
        .column_by_role(&ColumnRole::Time)
        .unwrap()
        .f64()
        .unwrap();
    assert_eq!(
        times.iter().filter(|value| *value == Some(807.0)).count(),
        2
    );
    assert!(
        dataset
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::DuplicateTime)
    );
    assert_eq!(times.get(0), Some(0.1));
    assert_eq!(times.get(10_000), Some(1000.0));
    assert!((value(&dataset, "ch1_v", 0) + 0.09630).abs() < 1e-12);
    assert!((value(&dataset, "ch5_v", 0) + 0.1051).abs() < 1e-12);
    assert!((value(&dataset, "ch1_v", 10_000) - 0.06745).abs() < 1e-12);
    assert!((value(&dataset, "ch5_v", 10_000) - 0.06092).abs() < 1e-12);
}

#[test]
fn supplied_eis_sign_and_descending_frequency_are_preserved() {
    if !fixture("EIS.csv").exists() {
        return;
    }
    let options = ReadOptions::new().with_validation(ValidationLevel::Scientific);
    let dataset = read_with_options(fixture("EIS.csv"), &options).unwrap();
    assert_eq!(dataset.format, FormatId::ChiEis);
    assert_eq!(dataset.data.shape(), (84, 5));
    assert_eq!(
        dataset.data.get_column_names(),
        [
            "frequency_hz",
            "z_real_ohm",
            "z_imag_ohm",
            "z_magnitude_ohm",
            "phase_deg"
        ]
    );
    assert_eq!(
        dataset
            .columns
            .iter()
            .map(|c| c.role.clone())
            .collect::<Vec<_>>(),
        vec![
            ColumnRole::Frequency,
            ColumnRole::ImpedanceReal,
            ColumnRole::ImpedanceImaginary,
            ColumnRole::ImpedanceMagnitude,
            ColumnRole::Phase
        ]
    );
    assert_eq!(value(&dataset, "frequency_hz", 0), 82_520.0);
    assert_eq!(value(&dataset, "frequency_hz", 83), 0.01);
    assert_eq!(value(&dataset, "z_real_ohm", 0), 433.0);
    assert_eq!(value(&dataset, "z_magnitude_ohm", 83), 14_390_000.0);
    assert_eq!(value(&dataset, "phase_deg", 83), -79.6);
    assert!(value(&dataset, "z_imag_ohm", 0) < 0.0);
    assert!(value(&dataset, "z_imag_ohm", 83) < 0.0);
    assert!(
        dataset
            .metadata
            .parameters
            .iter()
            .any(|p| p.raw_name == "Imp SF")
    );
}
