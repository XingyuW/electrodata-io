//! Date-time time columns in generic tables and workbooks: they become seconds
//! elapsed since the first row, with the absolute start kept as `TimeOrigin`.

use electrodata_io::{
    ColumnRole, DateOrder, DiagnosticCode, Error, InvalidTimePolicy, ReadOptions, ReadProfile,
    Severity, TimestampFormat, Unit, read_with_options,
};
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};
use std::io::Write;

fn csv(text: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file
}

fn compatibility() -> ReadOptions {
    ReadOptions::new().with_profile(ReadProfile::Compatibility)
}

fn read(text: &str) -> electrodata_io::Dataset {
    let file = csv(text);
    read_with_options(file.path(), &compatibility()).unwrap()
}

fn time(dataset: &electrodata_io::Dataset) -> Vec<f64> {
    dataset
        .f64_values(&ColumnRole::Time)
        .unwrap()
        .iter()
        .map(|value| value.unwrap())
        .collect()
}

fn assert_times(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-9, "{actual} vs {expected}");
    }
}

fn notes(dataset: &electrodata_io::Dataset) -> Vec<(Severity, String)> {
    dataset
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::TimestampInterpretation)
        .map(|diagnostic| (diagnostic.severity, diagnostic.message.clone()))
        .collect()
}

#[test]
fn every_supported_text_form_gives_the_same_axis() {
    for (header, rows) in [
        (
            "Timestamp",
            [
                "2026-11-17T09:30:00Z",
                "2026-11-17T09:30:01.5Z",
                "2026-11-17T09:31:00Z",
            ],
        ),
        (
            "time",
            [
                "2026-11-17 09:30:00",
                "2026-11-17 09:30:01.500",
                "2026-11-17 09:31:00",
            ],
        ),
        (
            "Date/Time",
            [
                "2026/11/17 09:30:00",
                "2026/11/17 09:30:01.5",
                "2026/11/17 09:31:00",
            ],
        ),
        (
            "Date Time",
            [
                "17.11.2026 09:30:00",
                "17.11.2026 09:30:01,5",
                "17.11.2026 09:31:00",
            ],
        ),
        (
            "DateTime",
            [
                "17-Nov-2026 09:30:00",
                "17-Nov-2026 09:30:01.5",
                "17-Nov-2026 09:31:00",
            ],
        ),
        (
            "Time",
            [
                "11/17/2026 9:30:00 AM",
                "11/17/2026 9:30:01.5 AM",
                "11/17/2026 9:31:00 AM",
            ],
        ),
        (
            "Time",
            [
                "17/11/2026 09:30:00",
                "17/11/2026 09:30:01.5",
                "17/11/2026 09:31:00",
            ],
        ),
        ("Time", ["09:30:00", "09:30:01.5", "09:31:00"]),
    ] {
        let separator = if rows[0].contains(',') || rows[1].contains(',') {
            ';'
        } else {
            ','
        };
        let text = format!(
            "{header}{separator}E1/V\n{}{separator}0.1\n{}{separator}0.2\n{}{separator}0.3\n",
            rows[0], rows[1], rows[2]
        );
        let dataset = read(&text);
        assert_times(&time(&dataset), &[0.0, 1.5, 60.0]);
        let view = dataset.time_series_view().unwrap();
        assert_eq!(view.time.unit, Some(Unit::Second), "{header}");
        assert_eq!(view.measurements.len(), 1, "{header}");
        assert_eq!(view.time_seconds.unwrap().len(), 3);
    }
}

#[test]
fn the_origin_records_the_first_row() {
    let dataset = read(
        "Timestamp,E1/V\n2026-11-17T09:30:00.250+01:00,0.1\n2026-11-17T09:30:01.250+01:00,0.2\n",
    );
    let acquisition = &dataset.metadata.acquisition;
    let origin = acquisition.time_origin.as_ref().unwrap();
    assert_eq!(origin.format, TimestampFormat::YearFirst);
    assert_eq!(origin.source_row, 2);
    assert_eq!(origin.source_text, "2026-11-17T09:30:00.250+01:00");
    assert_eq!(origin.utc_offset_seconds, Some(3600));
    assert_eq!(
        origin.iso8601().as_deref(),
        Some("2026-11-17T09:30:00.250+01:00")
    );
    assert_eq!(acquisition.recorded_at, origin.date_time);
}

#[test]
fn offsets_are_compared_as_instants() {
    // The clocks change from +02:00 to +01:00 between these rows: the wall
    // clock repeats, but one hour and one second have really passed.
    let dataset =
        read("Timestamp,E1/V\n2026-10-25T02:59:59+02:00,0.1\n2026-10-25T02:00:00+01:00,0.2\n");
    assert_times(&time(&dataset), &[0.0, 1.0]);
}

#[test]
fn precision_survives_calendar_sized_values() {
    let dataset =
        read("Timestamp,E1/V\n2026-11-17T09:30:00.000001Z,0.1\n2026-11-17T09:30:00.000002Z,0.2\n");
    assert_eq!(time(&dataset), vec![0.0, 1e-6]);
}

#[test]
fn numeric_dates_are_settled_by_the_column_or_refused() {
    // A day above 12 settles the order, even late in the column.
    let day_first = read(
        "Time,E1/V\n12/11/2026 23:59:59,0.1\n13/11/2026 00:00:00,0.2\n14/11/2026 00:00:00,0.3\n",
    );
    assert_times(&time(&day_first), &[0.0, 1.0, 86_401.0]);
    assert_eq!(
        day_first
            .metadata
            .acquisition
            .time_origin
            .as_ref()
            .unwrap()
            .format,
        TimestampFormat::DayFirst
    );
    let month_first = read("Time,E1/V\n11/12/2026 23:59:59,0.1\n11/13/2026 00:00:00,0.2\n");
    assert_times(&time(&month_first), &[0.0, 1.0]);

    // 03/04 to 04/04 is one day or one month depending on the order.
    let ambiguous = "Time,E1/V\n03/04/2026 12:00:00,0.1\n04/04/2026 12:00:00,0.2\n";
    let file = csv(ambiguous);
    let error = read_with_options(file.path(), &compatibility()).unwrap_err();
    assert!(error.to_string().contains("date order"), "{error}");
    assert_eq!(error.row(), Some(2));
    let stated = |order| {
        let options = compatibility().with_date_order(order);
        time(&read_with_options(file.path(), &options).unwrap())
    };
    assert_times(&stated(DateOrder::DayFirst), &[0.0, 86_400.0]);
    assert_times(&stated(DateOrder::MonthFirst), &[0.0, 31.0 * 86_400.0]);

    // Rows that contradict each other, or the stated order, are refused.
    let file = csv("Time,E1/V\n13/11/2026 00:00:00,0.1\n11/13/2026 00:00:01,0.2\n");
    let error = read_with_options(file.path(), &compatibility()).unwrap_err();
    assert!(matches!(root(&error), Error::AmbiguousDateOrder { .. }));
    let file = csv("Time,E1/V\n13/11/2026 00:00:00,0.1\n14/11/2026 00:00:01,0.2\n");
    let options = compatibility().with_date_order(DateOrder::MonthFirst);
    assert!(read_with_options(file.path(), &options).is_err());
}

fn root(error: &Error) -> &Error {
    match error {
        Error::ReadContext { source, .. } => root(source),
        other => other,
    }
}

#[test]
fn one_ambiguous_date_keeps_elapsed_time_but_not_the_date() {
    let dataset = read("Time,E1/V\n03/04/2026 12:00:00,0.1\n03/04/2026 12:00:05,0.2\n");
    assert_times(&time(&dataset), &[0.0, 5.0]);
    let origin = dataset.metadata.acquisition.time_origin.clone().unwrap();
    assert_eq!(origin.format, TimestampFormat::UnresolvedDateOrder);
    assert_eq!(origin.date_time, None);
    assert_eq!(origin.iso8601(), None);
    assert_eq!(dataset.metadata.acquisition.recorded_at, None);
    assert!(
        notes(&dataset)
            .iter()
            .any(|(severity, message)| *severity == Severity::Warning
                && message.contains("calendar date is not recorded"))
    );
}

#[test]
fn clock_columns_cross_midnight_and_durations_do_not_wrap() {
    let clock = read("Time,E1/V\n23:59:59,0.1\n00:00:01,0.2\n00:00:00,0.3\n");
    // The last row is a small backward step, not another midnight.
    assert_times(&time(&clock), &[0.0, 2.0, 1.0]);
    assert!(
        notes(&clock)
            .iter()
            .any(|(_, message)| message.contains("1 midnight rollover"))
    );
    assert!(
        clock
            .diagnostics
            .iter()
            .any(|item| item.code == DiagnosticCode::NonMonotonicTimestamp)
    );
    let origin = clock.metadata.acquisition.time_origin.as_ref().unwrap();
    assert_eq!(origin.format, TimestampFormat::ClockTime);
    assert_eq!(origin.date_time, None);

    let elapsed = read("Time,E1/V\n23:59:59,0.1\n24:00:00,0.2\n36:00:00,0.3\n");
    assert_times(&time(&elapsed), &[0.0, 1.0, 43_201.0]);
    assert_eq!(
        elapsed
            .metadata
            .acquisition
            .time_origin
            .as_ref()
            .unwrap()
            .format,
        TimestampFormat::ElapsedDuration
    );
}

#[test]
fn separate_date_and_time_columns_are_combined() {
    let dataset = read(
        "Date,Time,E1/V\n2026-11-17,23:59:59,0.1\n2026-11-18,00:00:01,0.2\n2026-11-18,00:00:02,\n",
    );
    assert_times(&time(&dataset), &[0.0, 2.0, 3.0]);
    let view = dataset.time_series_view().unwrap();
    assert_eq!(view.measurements.len(), 1);
    assert_eq!(view.measurements[0].original_name.as_deref(), Some("E1/V"));
    // Only the real blank counts as missing, not the consumed date column.
    assert_eq!(dataset.diagnostic_summary().missing_cells, 1);
}

#[test]
fn a_numeric_time_column_stays_the_coordinate() {
    let dataset =
        read("Timestamp,Time/s,E1/V\n2026-11-17 09:30:00,10,0.1\n2026-11-17 09:30:01,11,0.2\n");
    assert_eq!(time(&dataset), vec![10.0, 11.0]);
    let view = dataset.time_series_view().unwrap();
    assert_eq!(view.measurements.len(), 1);
    assert_eq!(dataset.metadata.acquisition.time_origin, None);
}

#[test]
fn bad_rows_follow_the_coordinate_policy() {
    let text = "Time,E1/V\n2026-11-17 09:30:00,0.1\n2026-02-30 09:30:01,0.2\n42,0.3\n17.11.2026 09:30:03,0.4\n2026-11-17 09:30:04,0.5\n";
    let dataset = read(text);
    assert_times(&time(&dataset), &[0.0, 4.0]);
    assert_eq!(dataset.diagnostic_summary().skipped_rows, 3);
    assert_eq!(dataset.source_row(1), Some(6));
    let file = csv(text);
    let strict = compatibility().with_invalid_time_policy(InvalidTimePolicy::Error);
    let error = read_with_options(file.path(), &strict).unwrap_err();
    assert_eq!(error.row(), Some(3));
}

#[test]
fn numeric_time_columns_are_untouched() {
    let dataset = read("time/s,E1/V\n0,0.1\n1,0.2\n2,0.3\n");
    assert_eq!(time(&dataset), vec![0.0, 1.0, 2.0]);
    assert_eq!(dataset.metadata.acquisition.time_origin, None);
    assert!(notes(&dataset).is_empty());
    // A numeric column headed Date remains a measurement.
    let dated = read("time/s,Date\n0,20261117\n1,20261118\n");
    let view = dated.time_series_view().unwrap();
    assert_eq!(view.measurements[0].values[0], Some(20_261_117.0));
    // So does a numeric column headed like a date-time.
    let serial = read("time/s,DateTime,E1/V\n0,46343.5,0.1\n1,46343.6,0.2\n");
    assert_eq!(time(&serial), vec![0.0, 1.0]);
    let view = serial.time_series_view().unwrap();
    assert_eq!(view.measurements.len(), 2);
    assert_eq!(view.measurements[0].values[0], Some(46_343.5));
}

fn workbook(write: impl FnOnce(&mut rust_xlsxwriter::Worksheet)) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".xlsx").tempfile().unwrap();
    let mut workbook = Workbook::new();
    write(workbook.add_worksheet());
    workbook.save(file.path()).unwrap();
    file
}

#[test]
fn workbook_date_cells_become_elapsed_seconds() {
    let format = Format::new().set_num_format("yyyy-mm-dd hh:mm:ss.000");
    let file = workbook(|sheet| {
        sheet.write(0, 0, "Time").unwrap();
        sheet.write(0, 1, "E1/V").unwrap();
        for (row, text) in [
            "2026-11-17T09:30:00",
            "2026-11-17T09:30:01.500",
            "2026-11-18T09:30:00",
        ]
        .iter()
        .enumerate()
        {
            let value = ExcelDateTime::parse_from_str(text).unwrap();
            sheet
                .write_datetime_with_format(row as u32 + 1, 0, &value, &format)
                .unwrap();
            sheet.write(row as u32 + 1, 1, 0.1).unwrap();
        }
    });
    let dataset = read_with_options(file.path(), &compatibility()).unwrap();
    assert_times(&time(&dataset), &[0.0, 1.5, 86_400.0]);
    let origin = dataset.metadata.acquisition.time_origin.unwrap();
    assert_eq!(origin.iso8601().as_deref(), Some("2026-11-17T09:30:00"));
}

#[test]
fn workbook_duration_and_clock_cells_are_read() {
    for (pattern, expected_format) in [
        ("[h]:mm:ss", TimestampFormat::ElapsedDuration),
        ("hh:mm:ss", TimestampFormat::ClockTime),
    ] {
        let format = Format::new().set_num_format(pattern);
        let file = workbook(|sheet| {
            sheet.write(0, 0, "Time").unwrap();
            sheet.write(0, 1, "E1/V").unwrap();
            for row in 0..3_u32 {
                let days = f64::from(row) * 60.0 / 86_400.0;
                sheet
                    .write_number_with_format(row + 1, 0, days, &format)
                    .unwrap();
                sheet.write(row + 1, 1, 0.1).unwrap();
            }
        });
        let dataset = read_with_options(file.path(), &compatibility()).unwrap();
        assert_times(&time(&dataset), &[0.0, 60.0, 120.0]);
        assert_eq!(
            dataset.metadata.acquisition.time_origin.unwrap().format,
            expected_format
        );
    }
}

#[test]
fn workbook_date_and_time_columns_and_text_are_read() {
    let date = Format::new().set_num_format("yyyy-mm-dd");
    let clock = Format::new().set_num_format("hh:mm:ss");
    let file = workbook(|sheet| {
        for (column, name) in ["Date", "Time", "E1/V"].iter().enumerate() {
            sheet.write(0, column as u16, *name).unwrap();
        }
        for (row, (day, seconds)) in [("2026-11-17", 86_399.0), ("2026-11-18", 1.0)]
            .iter()
            .enumerate()
        {
            let row = row as u32 + 1;
            let value = ExcelDateTime::parse_from_str(day).unwrap();
            sheet
                .write_datetime_with_format(row, 0, &value, &date)
                .unwrap();
            sheet
                .write_number_with_format(row, 1, seconds / 86_400.0, &clock)
                .unwrap();
            sheet.write(row, 2, 0.1).unwrap();
        }
    });
    let dataset = read_with_options(file.path(), &compatibility()).unwrap();
    assert_times(&time(&dataset), &[0.0, 2.0]);
    assert_eq!(dataset.time_series_view().unwrap().measurements.len(), 1);
    let origin = dataset.metadata.acquisition.time_origin.clone().unwrap();
    assert_eq!(origin.iso8601().as_deref(), Some("2026-11-17T23:59:59"));

    let file = workbook(|sheet| {
        sheet.write(0, 0, "Run notes").unwrap();
        sheet.write(1, 0, "Timestamp").unwrap();
        sheet.write(1, 1, "E1/V").unwrap();
        sheet.write(2, 0, "2026-11-17 09:30:00").unwrap();
        sheet.write(2, 1, 0.1).unwrap();
        sheet.write(3, 0, "2026-11-17 09:30:02").unwrap();
        sheet.write(3, 1, 0.2).unwrap();
    });
    let dataset = read_with_options(file.path(), &compatibility()).unwrap();
    assert_times(&time(&dataset), &[0.0, 2.0]);
}

#[test]
fn a_date_format_on_a_measurement_cell_keeps_its_number() {
    let format = Format::new().set_num_format("yyyy-mm-dd");
    let file = workbook(|sheet| {
        sheet.write(0, 0, "time/s").unwrap();
        sheet.write(0, 1, "E1/V").unwrap();
        for row in 1..3_u32 {
            sheet.write(row, 0, f64::from(row)).unwrap();
            sheet
                .write_number_with_format(row, 1, 1.5, &format)
                .unwrap();
        }
    });
    let dataset = read_with_options(file.path(), &compatibility()).unwrap();
    let view = dataset.time_series_view().unwrap();
    assert_eq!(view.measurements[0].values[0], Some(1.5));
    assert_eq!(view.time.values[0], Some(1.0));
}
