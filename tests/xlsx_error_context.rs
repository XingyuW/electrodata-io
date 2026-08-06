use electrodata_io::{Error, ErrorCategory, ReadOptions, SheetSelector, read, read_with_options};
use rust_xlsxwriter::Workbook;
use std::error::Error as StdError;

fn write_regular_sheet(workbook: &mut Workbook, name: &str, bad_time: bool, bad_value: bool) {
    let sheet = workbook.add_worksheet();
    sheet.set_name(name).unwrap();
    sheet.write_string(0, 0, "Time/s").unwrap();
    sheet.write_string(0, 1, "Potential/V").unwrap();
    sheet.write_number(1, 0, 0.0).unwrap();
    sheet.write_number(1, 1, 0.1).unwrap();
    if bad_time {
        sheet.write_string(2, 0, "not-a-time").unwrap();
    } else {
        sheet.write_number(2, 0, 1.0).unwrap();
    }
    if bad_value {
        sheet.write_string(2, 1, "not-a-number").unwrap();
    } else {
        sheet.write_number(2, 1, 0.2).unwrap();
    }
}

fn write_chi_sheet_with_bad_value(workbook: &mut Workbook, name: &str) {
    let sheet = workbook.add_worksheet();
    sheet.set_name(name).unwrap();
    sheet
        .write_string(0, 0, "Open Circuit Potential - Time")
        .unwrap();
    sheet.write_string(1, 0, "Time/s").unwrap();
    sheet.write_string(1, 1, "Potential/V").unwrap();
    sheet.write_number(2, 0, 0.0).unwrap();
    sheet.write_number(2, 1, 0.1).unwrap();
    sheet.write_number(3, 0, 1.0).unwrap();
    sheet.write_number(3, 1, 0.2).unwrap();
    sheet.write_number(4, 0, 2.0).unwrap();
    sheet.write_number(4, 1, 0.3).unwrap();
    sheet.write_number(5, 0, 3.0).unwrap();
    sheet.write_string(5, 1, "not-a-number").unwrap();
}

fn assert_worksheet_context(
    error: &Error,
    path: &std::path::Path,
    worksheet: &str,
    index: usize,
    category: ErrorCategory,
) {
    assert_eq!(error.path(), Some(path));
    assert_eq!(error.worksheet(), Some(worksheet));
    assert_eq!(error.worksheet_index(), Some(index));
    let context = error.context();
    assert_eq!(context.path, Some(path));
    assert_eq!(context.worksheet, Some(worksheet));
    assert_eq!(context.worksheet_index, Some(index));
    assert_eq!(context.category, category);
    assert!(error.source_error().is_some());
}

#[test]
fn explicit_name_preserves_context_and_structured_malformed_numeric_source() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_regular_sheet(&mut workbook, "SheetA", false, true);
    workbook.save(file.path()).unwrap();

    let error = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Name("SheetA".into())),
    )
    .unwrap_err();
    assert_worksheet_context(
        &error,
        file.path(),
        "SheetA",
        0,
        ErrorCategory::MalformedNumeric,
    );
    assert_eq!((error.row(), error.column()), (Some(3), Some(2)));
    let Error::ReadContext { source, .. } = &error else {
        panic!("selected worksheet error must use ReadContext: {error:?}");
    };
    assert!(matches!(
        source.as_ref(),
        Error::MalformedNumeric {
            row: 3,
            column: 2,
            value,
            ..
        } if value == "not-a-number"
    ));
    assert!(StdError::source(&error).is_some());
}

#[test]
fn invalid_time_value_preserves_selected_worksheet() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_regular_sheet(&mut workbook, "SheetA", true, false);
    workbook.save(file.path()).unwrap();

    let error = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Name("SheetA".into())),
    )
    .unwrap_err();
    assert_worksheet_context(
        &error,
        file.path(),
        "SheetA",
        0,
        ErrorCategory::MalformedNumeric,
    );
    assert_eq!((error.row(), error.column()), (Some(3), Some(1)));
    assert!(matches!(
        &error,
        Error::ReadContext {
            source,
            ..
        } if matches!(source.as_ref(), Error::MalformedNumeric { .. })
    ));
}

#[test]
fn typed_view_failure_uses_xlsx_dataset_provenance() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_regular_sheet(&mut workbook, "SheetA", false, false);
    workbook.save(file.path()).unwrap();
    let dataset = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Name("SheetA".into())),
    )
    .unwrap();
    assert_eq!(dataset.metadata.provenance.worksheet_index, Some(0));

    let error = dataset.eis_view().unwrap_err();
    assert_worksheet_context(&error, file.path(), "SheetA", 0, ErrorCategory::Accessor);
    assert!(matches!(
        &error,
        Error::ReadContext {
            source,
            ..
        } if matches!(source.as_ref(), Error::InvalidDatasetView { .. })
    ));
}

#[test]
fn explicit_index_preserves_resolved_name_and_index() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_regular_sheet(&mut workbook, "First", false, false);
    write_regular_sheet(&mut workbook, "SheetA", false, true);
    workbook.save(file.path()).unwrap();

    let error = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Index(1)),
    )
    .unwrap_err();
    assert_worksheet_context(
        &error,
        file.path(),
        "SheetA",
        1,
        ErrorCategory::MalformedNumeric,
    );
}

#[test]
fn automatic_unique_worksheet_preserves_context_after_selection() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let notes = workbook.add_worksheet();
    notes.set_name("Notes").unwrap();
    notes.write_string(0, 0, "not scientific data").unwrap();
    write_regular_sheet(&mut workbook, "Data", false, true);
    workbook.save(file.path()).unwrap();

    let error = read(file.path()).unwrap_err();
    assert_worksheet_context(
        &error,
        file.path(),
        "Data",
        1,
        ErrorCategory::MalformedNumeric,
    );
}

#[test]
fn chi_parser_failure_preserves_selected_worksheet_without_diagnostics() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write_chi_sheet_with_bad_value(&mut workbook, "ocpt");
    workbook.save(file.path()).unwrap();

    let error = read_with_options(
        file.path(),
        &ReadOptions::new().with_sheet(SheetSelector::Name("ocpt".into())),
    )
    .unwrap_err();
    assert_worksheet_context(
        &error,
        file.path(),
        "ocpt",
        0,
        ErrorCategory::MalformedNumeric,
    );
    assert!(error.diagnostics().is_empty());
    assert!(matches!(
        &error,
        Error::ReadContext {
            source,
            ..
        } if matches!(source.as_ref(), Error::MalformedNumeric { row: 6, column: 2, .. })
    ));
}

#[test]
fn xlsx_eis_required_role_failure_preserves_selected_worksheet() {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("EIS").unwrap();
    sheet.write_string(0, 0, "Freq/Hz").unwrap();
    sheet.write_string(0, 1, "Z'/ohm").unwrap();
    sheet.write_number(1, 0, 1000.0).unwrap();
    sheet.write_number(1, 1, 10.0).unwrap();
    workbook.save(file.path()).unwrap();

    let error = read_with_options(
        file.path(),
        &ReadOptions::new()
            .with_sheet(SheetSelector::Name("EIS".into()))
            .with_format(electrodata_io::FormatId::ChiEis),
    )
    .unwrap_err();
    assert_worksheet_context(&error, file.path(), "EIS", 0, ErrorCategory::Schema);
    assert!(matches!(
        &error,
        Error::ReadContext {
            source,
            ..
        } if matches!(source.as_ref(), Error::MissingRequiredRole { .. })
    ));
}
