use electrodata_io::*;
use rust_xlsxwriter::Workbook;
use std::path::Path;

fn temp_csv(text: &str) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(file.path(), text).unwrap();
    file
}

fn chi(header: &str, metadata: &str, rows: &str) -> String {
    format!("Mar 6, 2026   13:55:19\nOpen Circuit Potential - Time\n{metadata}{header}\n{rows}")
}

#[test]
fn regular_header_and_headerless_keep_all_rows() {
    let with = temp_csv("x,y\n1,2\n3e0,4e0\n");
    let without = temp_csv("1,2\n3e0,4e0\n5,6\n");
    assert_eq!(read(with.path()).unwrap().data.shape(), (2, 2));
    let dataset = read(without.path()).unwrap();
    assert_eq!(dataset.data.shape(), (3, 2));
    assert_eq!(
        dataset.data.column("x").unwrap().f64().unwrap().get(0),
        Some(1.0)
    );
}

#[test]
fn forced_header_and_source_names_work() {
    let file = temp_csv("left,right\n1,2\n3,4\n");
    let options = ReadOptions::new()
        .with_header_policy(HeaderPolicy::Present)
        .with_column_name_policy(ColumnNamePolicy::PreserveSource)
        .with_format(FormatId::RegularXy);
    let dataset = read_with_options(file.path(), &options).unwrap();
    assert_eq!(dataset.data.get_column_names(), ["left", "right"]);
}

#[test]
fn numbered_channels_support_one_eight_noncontiguous_and_both_prefixes() {
    for header in [
        "Time/s,E1/V",
        "Time/s,CH1,CH2,CH3,CH4,CH5,CH6,CH7,CH8",
        "Time/s,E1/V,E5/V",
        " Time/s , ch1/v , CH5 ",
    ] {
        let width = header.split(',').count();
        let row = (0..width)
            .map(|index| (index + 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let file = temp_csv(&chi(header, "Unknown metadata row\nFlag X\n", &row));
        let dataset = read(file.path()).unwrap();
        let expected = if width == 2 {
            FormatId::ChiOcpt
        } else {
            FormatId::ChiMultichannelOcpt
        };
        assert_eq!(dataset.format, expected);
        assert_eq!(dataset.data.width(), width);
        assert!(
            dataset
                .metadata
                .parameters
                .iter()
                .any(|p| p.raw_name == "Flag X")
        );
    }
}

#[test]
fn blank_separator_counts_bom_and_line_endings_work() {
    for separator in ["", "\n", "\n\n\n"] {
        let text = format!(
            "\u{feff}Mar. 6, 2026   13:55:19\r\nOpen Circuit Potential - Time\r\nTime/sec,Potential/V\r\n{separator}1e-1,2e-1\r\n2e-1,3e-1\r\n"
        );
        let file = temp_csv(&text);
        let dataset = read(file.path()).unwrap();
        assert_eq!(dataset.data.shape(), (2, 2));
    }
}

#[test]
fn missing_cells_are_null_but_malformed_and_ragged_rows_fail() {
    let missing = temp_csv("x,y\n1,\n2,3\n");
    assert_eq!(
        read(missing.path())
            .unwrap()
            .data
            .column("y")
            .unwrap()
            .null_count(),
        1
    );
    let malformed = temp_csv("x,y\n1,nope\n2,3\n");
    assert!(matches!(
        read(malformed.path()),
        Err(Error::MalformedNumeric {
            row: 2,
            column: 2,
            ..
        })
    ));
    let ragged = temp_csv("x,y\n1,2\n3,4,5\n");
    assert!(matches!(
        read(ragged.path()),
        Err(Error::RaggedRow { row: 3, .. })
    ));
    let empty = temp_csv("");
    assert!(read(empty.path()).is_err());
}

struct Custom;
impl FormatHandler for Custom {
    fn format_id(&self) -> FormatId {
        FormatId::Custom("custom-two".into())
    }
    fn detect(&self, probe: &Probe, _options: &ReadOptions) -> Option<Detection> {
        (probe.row(0)?.first()?.trim() == "CUSTOM").then(|| Detection {
            format: self.format_id(),
            confidence: 1.0,
            evidence: vec!["custom marker".into()],
            layout: DetectedLayout {
                header_row: Some(1),
                data_start_row: 2,
                metadata_range: Some(0..1),
                expected_column_count: 2,
                has_header: true,
            },
        })
    }
    fn columns(&self, _probe: &Probe, _detection: &Detection) -> Result<Vec<ColumnDescriptor>> {
        Ok(vec![
            ColumnDescriptor {
                source_name: "A".into(),
                canonical_name: "a".into(),
                role: ColumnRole::Custom("a".into()),
                unit: None,
            },
            ColumnDescriptor {
                source_name: "B".into(),
                canonical_name: "b".into(),
                role: ColumnRole::Custom("b".into()),
                unit: None,
            },
        ])
    }
}

#[test]
fn custom_handler_registration_uses_the_shared_pipeline() {
    let file = temp_csv("CUSTOM\nA,B\n1,2\n3,4\n");
    let reader = DataReader::builder()
        .with_builtin_formats()
        .register(Custom)
        .build();
    let dataset = reader.read(file.path()).unwrap();
    assert_eq!(dataset.format, FormatId::Custom("custom-two".into()));
    assert_eq!(dataset.data.shape(), (2, 2));
}

fn write_regular_sheet(workbook: &mut Workbook, name: &str, value: f64) {
    let sheet = workbook.add_worksheet();
    sheet.set_name(name).unwrap();
    sheet.write_string(0, 0, "x").unwrap();
    sheet.write_string(0, 1, "y").unwrap();
    sheet.write_number(1, 0, value).unwrap();
    sheet.write_number(1, 1, value + 1.0).unwrap();
    sheet.write_number(2, 0, value + 2.0).unwrap();
    sheet.write_number(2, 1, value + 3.0).unwrap();
}

#[test]
fn excel_sheet_selection_auto_explicit_and_ambiguous() {
    let directory = tempfile::tempdir().unwrap();
    let selected_path = directory.path().join("selected.xlsx");
    let mut workbook = Workbook::new();
    let notes = workbook.add_worksheet();
    notes.set_name("Notes").unwrap();
    notes.write_string(0, 0, "not data").unwrap();
    write_regular_sheet(&mut workbook, "Data", 1.0);
    workbook.save(&selected_path).unwrap();
    let dataset = read(&selected_path).unwrap();
    assert_eq!(
        dataset.metadata.provenance.worksheet.as_deref(),
        Some("Data")
    );
    let explicit = ReadOptions::new().with_sheet(SheetSelector::Name("Data".into()));
    assert_eq!(
        read_with_options(&selected_path, &explicit)
            .unwrap()
            .data
            .shape(),
        (2, 2)
    );

    let ambiguous_path = directory.path().join("ambiguous.xlsx");
    let mut workbook = Workbook::new();
    write_regular_sheet(&mut workbook, "One", 1.0);
    write_regular_sheet(&mut workbook, "Two", 10.0);
    workbook.save(&ambiguous_path).unwrap();
    assert!(matches!(
        read(&ambiguous_path),
        Err(Error::AmbiguousDetection { .. }) | Err(Error::AmbiguousFormat { .. })
    ));
}

#[test]
fn empty_workbook_fails_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.xlsx");
    Workbook::new().save(&path).unwrap();
    assert!(read(Path::new(&path)).is_err());
}
