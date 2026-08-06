use electrodata_io::{ColumnRole, DatasetKind, Error, read};
use rust_xlsxwriter::Workbook;
use std::path::{Path, PathBuf};

fn consumer_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rust_electroanalysis_cli/single_timeseries_with_preamble.xlsx")
}

fn workbook_with_preamble(rows: &[Option<&str>]) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("measurement").unwrap();
    for (row, text) in rows.iter().enumerate() {
        if let Some(text) = text {
            sheet.write_string(row as u32, 0, *text).unwrap();
        }
    }
    let header = rows.len() as u32;
    sheet.write_string(header, 0, "Time/s").unwrap();
    sheet.write_string(header, 1, "Potential/V").unwrap();
    sheet.write_number(header + 1, 0, 0.0).unwrap();
    sheet.write_number(header + 1, 1, 0.215).unwrap();
    sheet.write_number(header + 2, 0, 1.0).unwrap();
    sheet.write_number(header + 2, 1, 0.217).unwrap();
    sheet.write_number(header + 3, 0, 2.0).unwrap();
    workbook.save(file.path()).unwrap();
    file
}

#[test]
fn workbook_time_headers_are_discovered_after_evidence_based_preambles() {
    for preamble in [
        &[] as &[Option<&str>],
        &[Some("Experiment: NH4 ISM")],
        &[Some("Experiment: NH4 ISM"), Some("Operator: test")],
        &[Some("Experiment: NH4 ISM"), None, Some("Operator: test")],
    ] {
        let file = workbook_with_preamble(preamble);
        let dataset = read(file.path()).unwrap();
        assert_eq!(dataset.kind(), DatasetKind::TimeSeries);
        assert_eq!(dataset.canonical_columns()[0].role, ColumnRole::Time);
        assert_eq!(dataset.source_row(0), Some(preamble.len() + 2));
        assert_eq!(dataset.source_row(2), Some(preamble.len() + 4));
        assert_eq!(dataset.metadata.raw_rows.len(), preamble.len());
        assert_eq!(
            dataset.canonical_columns()[1].values,
            [Some(0.215), Some(0.217), None]
        );
    }
}

#[test]
fn historical_consumer_workbook_uses_the_normal_time_series_pipeline() {
    let dataset = read(consumer_fixture()).unwrap();
    assert_eq!(dataset.kind(), DatasetKind::TimeSeries);
    assert_eq!(dataset.canonical_columns()[0].role, ColumnRole::Time);
    assert_eq!(
        dataset.coordinate_values().unwrap(),
        [Some(0.0), Some(1.0), Some(2.0)]
    );
    assert_eq!(dataset.source_row(0), Some(5));
    assert_eq!(dataset.metadata.raw_rows.len(), 3);
    assert_eq!(
        dataset.metadata.raw_rows[0].reconstructed_text,
        "Experiment: NH4 ISM,,"
    );
}

#[test]
fn workbook_without_a_compatible_semantic_header_is_rejected() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write_string(0, 0, "Experiment notes").unwrap();
    sheet.write_string(1, 0, "Elapsed").unwrap();
    sheet.write_string(1, 1, "Value").unwrap();
    sheet.write_number(2, 0, 0.0).unwrap();
    sheet.write_number(2, 1, 1.0).unwrap();
    workbook.save(file.path()).unwrap();

    assert!(matches!(
        read(file.path()),
        Err(Error::NoCompatibleWorksheet { .. })
    ));
}
