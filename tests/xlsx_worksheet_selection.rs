use electrodata_io::{
    Error, ErrorCategory, FormatId, ReadOptions, SheetSelector, detect_format, read,
    read_with_options,
};
use rust_xlsxwriter::Workbook;

fn write_regular_sheet(workbook: &mut Workbook, name: &str, start: f64) {
    let sheet = workbook.add_worksheet();
    sheet.set_name(name).unwrap();
    sheet.write_string(0, 0, "Time/s").unwrap();
    sheet.write_string(0, 1, "Potential/V").unwrap();
    for row in 1..=3 {
        sheet
            .write_number(row, 0, start + f64::from(row - 1))
            .unwrap();
        sheet.write_number(row, 1, start + f64::from(row)).unwrap();
    }
}

fn write_ocpt_sheet(workbook: &mut Workbook, name: &str, start: f64) {
    let sheet = workbook.add_worksheet();
    sheet.set_name(name).unwrap();
    sheet
        .write_string(0, 0, "Open Circuit Potential - Time")
        .unwrap();
    sheet.write_string(1, 0, "Time/s").unwrap();
    sheet.write_string(1, 1, "Potential/V").unwrap();
    for row in 2..=4 {
        sheet
            .write_number(row, 0, start + f64::from(row - 2))
            .unwrap();
        sheet
            .write_number(row, 1, start + f64::from(row - 1))
            .unwrap();
    }
}

fn ambiguous_different_confidence_workbook() -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_ocpt_sheet(&mut workbook, "Sheet_HighConfidence", 1.0);
    write_regular_sheet(&mut workbook, "Sheet_LowerConfidence", 10.0);
    workbook.save(file.path()).unwrap();
    file
}

#[test]
fn automatic_read_rejects_different_confidence_compatible_worksheets() {
    let file = ambiguous_different_confidence_workbook();
    let error = read(file.path()).unwrap_err();
    let Error::AmbiguousWorksheet { path, candidates } = error else {
        panic!("expected worksheet ambiguity");
    };
    assert_eq!(path, file.path());
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].name, "Sheet_HighConfidence");
    assert_eq!(candidates[0].index, 0);
    assert_eq!(candidates[0].format, FormatId::ChiOcpt);
    assert!(!candidates[0].evidence.is_empty());
    assert_eq!(candidates[1].name, "Sheet_LowerConfidence");
    assert_eq!(candidates[1].index, 1);
    assert_eq!(candidates[1].format, FormatId::RegularXy);
    assert!(!candidates[1].evidence.is_empty());
    assert!(candidates[0].confidence > candidates[1].confidence);
}

#[test]
fn automatic_detect_rejects_different_confidence_compatible_worksheets() {
    let file = ambiguous_different_confidence_workbook();
    let error = detect_format(file.path()).unwrap_err();
    let Error::AmbiguousWorksheet { candidates, .. } = &error else {
        panic!("expected worksheet ambiguity");
    };
    assert_eq!(error.category(), ErrorCategory::Worksheet);
    assert_eq!(error.worksheet_candidates(), Some(candidates.as_slice()));
    assert_eq!(candidates.len(), 2);
    assert!(candidates[0].confidence > candidates[1].confidence);
}

#[test]
fn equal_confidence_worksheets_use_worksheet_ambiguity_context() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_regular_sheet(&mut workbook, "First", 1.0);
    write_regular_sheet(&mut workbook, "Second", 10.0);
    workbook.save(file.path()).unwrap();

    let error = read(file.path()).unwrap_err();
    let Error::AmbiguousWorksheet { candidates, .. } = error else {
        panic!("expected worksheet ambiguity");
    };
    assert!(
        candidates
            .iter()
            .all(|candidate| !matches!(candidate.format, FormatId::Custom(_)))
    );
}

#[test]
fn explicit_name_and_index_select_requested_compatible_worksheet() {
    let file = ambiguous_different_confidence_workbook();
    let high = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Name("Sheet_HighConfidence".into())),
    )
    .unwrap();
    assert_eq!(high.format, FormatId::ChiOcpt);
    assert_eq!(
        high.metadata.provenance.worksheet.as_deref(),
        Some("Sheet_HighConfidence")
    );

    let lower = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Index(1)),
    )
    .unwrap();
    assert_eq!(lower.format, FormatId::RegularXy);
    assert_eq!(
        lower.metadata.provenance.worksheet.as_deref(),
        Some("Sheet_LowerConfidence")
    );
}

#[test]
fn automatic_selection_uses_the_only_compatible_worksheet() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let notes = workbook.add_worksheet();
    notes.set_name("Notes").unwrap();
    notes.write_string(0, 0, "not scientific data").unwrap();
    write_regular_sheet(&mut workbook, "Data", 1.0);
    workbook.save(file.path()).unwrap();

    let dataset = read(file.path()).unwrap();
    assert_eq!(
        dataset.metadata.provenance.worksheet.as_deref(),
        Some("Data")
    );
}

#[test]
fn automatic_selection_reports_no_compatible_worksheet() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let notes = workbook.add_worksheet();
    notes.set_name("Notes").unwrap();
    notes.write_string(0, 0, "not scientific data").unwrap();
    workbook.save(file.path()).unwrap();

    let error = read(file.path()).unwrap_err();
    assert!(matches!(error, Error::NoCompatibleWorksheet { .. }));
    assert_eq!(error.category(), ErrorCategory::Worksheet);
    assert_eq!(error.path(), Some(file.path()));

    let error = detect_format(file.path()).unwrap_err();
    assert!(matches!(error, Error::NoCompatibleWorksheet { .. }));
}
