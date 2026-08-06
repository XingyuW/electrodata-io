use electrodata_io::{
    ColumnRole, ElectroDataFormat, Error, FormatId, ReadOptions, detect_format, read, read_chi_eis,
    read_chi_multichannel_ocpt, read_chi_ocpt, read_regular, read_with_options,
};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(name)
}

fn temporary_csv(contents: &str) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(file.path(), contents).unwrap();
    file
}

fn assert_equivalent(automatic: &electrodata_io::Dataset, explicit: &electrodata_io::Dataset) {
    assert_eq!(automatic.data.shape(), explicit.data.shape());
    assert_eq!(
        automatic.data.get_column_names(),
        explicit.data.get_column_names()
    );
    assert_eq!(automatic.columns, explicit.columns);
    assert_eq!(
        automatic.metadata.acquisition,
        explicit.metadata.acquisition
    );
    for name in automatic.data.get_column_names() {
        let automatic_column = automatic.data.column(name.as_str()).unwrap().f64().unwrap();
        let explicit_column = explicit.data.column(name.as_str()).unwrap().f64().unwrap();
        assert_eq!(automatic_column.get(0), explicit_column.get(0));
        let last = automatic.data.height() - 1;
        assert_eq!(automatic_column.get(last), explicit_column.get(last));
    }
}

#[test]
fn supplied_fixtures_detect_and_dispatch_to_equivalent_readers() {
    if !fixture("regular_file.csv").exists() {
        return;
    }
    let cases = [
        ("regular_file.csv", ElectroDataFormat::Regular),
        ("OCPT.csv", ElectroDataFormat::ChiOcpt),
        (
            "Multichannel_OCPT.csv",
            ElectroDataFormat::ChiMultichannelOcpt,
        ),
        ("EIS.csv", ElectroDataFormat::ChiEis),
    ];
    for (name, format) in cases {
        let path = fixture(name);
        let detection = detect_format(&path).unwrap();
        assert_eq!(detection.format, format, "{name}");
        let automatic = read_with_options(&path, &ReadOptions::compatibility()).unwrap();
        let explicit = read_with_options(
            &path,
            &ReadOptions::compatibility().with_format(format.into()),
        )
        .unwrap();
        assert_equivalent(&automatic, &explicit);
    }
}

#[test]
fn direct_builtin_readers_remain_available() {
    if !fixture("regular_file.csv").exists() {
        return;
    }
    assert_eq!(
        read_regular(fixture("regular_file.csv")).unwrap().format,
        FormatId::RegularXy
    );
    assert_eq!(
        read_chi_ocpt(fixture("OCPT.csv")).unwrap().format,
        FormatId::ChiOcpt
    );
    assert_eq!(
        read_chi_multichannel_ocpt(fixture("Multichannel_OCPT.csv"))
            .unwrap()
            .format,
        FormatId::ChiMultichannelOcpt
    );
    assert_eq!(
        read_chi_eis(fixture("EIS.csv")).unwrap().format,
        FormatId::ChiEis
    );
}

#[test]
fn alternative_eis_keyword_needs_a_complete_impedance_header() {
    let file = temporary_csv(
        "Electrochemical Impedance Spectroscopy\n\nFrequency/Hz,Z'/ohm,Z\"/ohm,Z/ohm,Phase/deg\n10,3,-4,5,-53.13\n1,6,-8,10,-53.13\n",
    );
    assert_eq!(
        detect_format(file.path()).unwrap().format,
        ElectroDataFormat::ChiEis
    );
    assert_eq!(read(file.path()).unwrap().format, FormatId::ChiEis);
}

#[test]
fn eis_technique_accepts_common_period_spacing_variations() {
    let file = temporary_csv(
        "A.C.Impedance\nFreq/Hz,Z'/ohm,Z\"/ohm,Z/ohm,Phase/deg\n10,3,-4,5,-53.13\n1,6,-8,10,-53.13\n",
    );
    let result = detect_format(file.path()).unwrap();
    assert_eq!(result.format, ElectroDataFormat::ChiEis);
    assert!(result.confidence >= 0.97);
}

#[test]
fn isolated_eis_word_does_not_override_a_regular_table() {
    let file = temporary_csv("EIS note,signal\n0,1\n1,2\n2,3\n");
    assert_eq!(
        detect_format(file.path()).unwrap().format,
        ElectroDataFormat::Regular
    );
}

#[test]
fn two_column_numbered_ocpt_is_ocpt_and_preserves_channel_role() {
    let file = temporary_csv(
        "Open-Circuit Potential\nFile: sample\nTime/second, E1/V\n0.0,0.1\n1.0,0.2\n",
    );
    let detected = detect_format(file.path()).unwrap();
    assert_eq!(detected.format, ElectroDataFormat::ChiOcpt);
    let dataset = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
    assert_eq!(dataset.format, FormatId::ChiOcpt);
    assert_eq!(dataset.columns[1].role, ColumnRole::PotentialChannel(1));
}

#[test]
fn multichannel_requires_two_or_more_channels_and_preserves_indices() {
    let file =
        temporary_csv("OCPT\nData Source: Experiment\nTime/s,CH1/V,E5/V,CH12\n0,1,2,3\n1,4,5,6\n");
    let detected = detect_format(file.path()).unwrap();
    assert_eq!(detected.format, ElectroDataFormat::ChiMultichannelOcpt);
    assert_eq!(detected.column_count, 4);
    let dataset = read(file.path()).unwrap();
    assert_eq!(dataset.columns[3].role, ColumnRole::PotentialChannel(12));
}

#[test]
fn multichannel_accepts_one_conventional_potential_plus_numbered_channels() {
    let file =
        temporary_csv("Open Circuit Potential - Time\nTime/s,Potential/V,E5/V\n0,1,2\n1,3,4\n");
    let dataset = read(file.path()).unwrap();
    assert_eq!(dataset.format, FormatId::ChiMultichannelOcpt);
    assert_eq!(dataset.columns[1].role, ColumnRole::Potential);
    assert_eq!(dataset.columns[2].role, ColumnRole::PotentialChannel(5));
    assert_eq!(
        dataset.data.get_column_names(),
        ["time_s", "potential_v", "ch5_v"]
    );
}

#[test]
fn ocpt_detection_tolerates_variable_metadata_preamble_lengths() {
    let file = temporary_csv(
        "Mar. 6, 2026   13:55:19\nOpen Circuit Potential - Time\nFile: sample.csv\nData Source: Experiment\nInstrument Model: CHI760F\nHeader:\nNote:\nCustom Flag\nRun Time (sec) = 4\n\nTime/sec,Potential/V\n\n0,0.1\n1,0.2\n",
    );
    let result = detect_format(file.path()).unwrap();
    assert_eq!(result.format, ElectroDataFormat::ChiOcpt);
    assert_eq!(result.header_row, Some(10));
    assert_eq!(result.data_start_row, 12);
}

#[test]
fn regular_headerless_bom_crlf_and_blank_lines_preserve_the_first_row() {
    let file = temporary_csv("\u{feff}\r\n\r\n0,1\r\n1,2\r\n2,3\r\n");
    let detected = detect_format(file.path()).unwrap();
    assert_eq!(detected.format, ElectroDataFormat::Regular);
    assert_eq!(detected.header_row, None);
    let dataset = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
    assert_eq!(dataset.data.height(), 3);
    assert_eq!(
        dataset.data.column("x").unwrap().f64().unwrap().get(0),
        Some(0.0)
    );
}

#[test]
fn one_row_regular_table_requires_explicit_selection() {
    let file = temporary_csv("x,y\n0,1\n");
    assert!(matches!(
        detect_format(file.path()),
        Err(Error::UnknownFormatDetailed { .. })
    ));
    let dataset = read_regular(file.path()).unwrap();
    assert_eq!(dataset.data.shape(), (1, 2));
}

#[test]
fn delimiter_detection_uses_the_bounded_structure_not_only_the_date_line() {
    let file = temporary_csv(
        "Mar. 6, 2026   13:55:19\nOpen Circuit Potential - Time\nFile: sample.csv\nTime/s;Potential/V\n0;0.1\n1;0.2\n",
    );
    let detection = detect_format(file.path()).unwrap();
    assert_eq!(detection.format, ElectroDataFormat::ChiOcpt);
    assert_eq!(read(file.path()).unwrap().data.shape(), (2, 2));
}

#[test]
fn malformed_scientific_headers_and_unknown_files_are_not_silently_classified() {
    let malformed = temporary_csv("Open Circuit Potential\nTime/s,Current/A\n0,1\n1,2\n");
    assert!(matches!(
        detect_format(malformed.path()),
        Err(Error::UnknownFormatDetailed { .. })
    ));
    let unknown = temporary_csv("instrument notes only\nno numeric table\n");
    assert!(matches!(
        detect_format(unknown.path()),
        Err(Error::UnknownFormatDetailed { .. })
    ));
}
