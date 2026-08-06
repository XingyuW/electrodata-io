use electrodata_io::{
    ColumnRole, DiagnosticCode, Error, ErrorCategory, FormatId, HeaderPolicy, ReadOptions,
    ReadProfile, RecoveryAction, Severity, ValidationLevel, read, read_with_options,
};

fn temporary_csv(contents: &str) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    std::fs::write(file.path(), contents).unwrap();
    file
}

#[derive(Debug, Clone)]
struct ExpectedTable {
    row_count: usize,
    first_source_row: usize,
    first_row: Vec<Option<f64>>,
    roles: Vec<ColumnRole>,
    missing_positions: Vec<(usize, usize)>,
    diagnostics: Vec<(DiagnosticCode, RecoveryAction, usize, Option<usize>)>,
}

fn assert_table(dataset: &electrodata_io::Dataset, expected: &ExpectedTable, profile: ReadProfile) {
    assert_eq!(dataset.data.height(), expected.row_count, "row count");
    assert!(
        dataset
            .canonical_columns()
            .iter()
            .all(|column| { column.values.len() == expected.row_count })
    );
    assert_eq!(dataset.source_row(0), Some(expected.first_source_row));
    assert_eq!(dataset.canonical_columns().len(), expected.roles.len());
    assert_eq!(
        dataset
            .canonical_columns()
            .iter()
            .map(|column| column.role.clone())
            .collect::<Vec<_>>(),
        expected.roles
    );
    assert_eq!(
        dataset
            .canonical_columns()
            .iter()
            .map(|column| column.values[0])
            .collect::<Vec<_>>(),
        expected.first_row,
        "first retained row"
    );
    let missing_positions = dataset
        .canonical_columns()
        .iter()
        .enumerate()
        .flat_map(|(column, values)| {
            values
                .values
                .iter()
                .enumerate()
                .filter_map(move |(row, value)| value.is_none().then_some((row, column)))
        })
        .collect::<Vec<_>>();
    assert_eq!(missing_positions, expected.missing_positions);
    assert_eq!(
        dataset
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.recovery,
                diagnostic.row.unwrap(),
                diagnostic.column_index,
            ))
            .collect::<Vec<_>>(),
        expected.diagnostics
    );
    assert!(
        dataset.diagnostics.iter().all(|diagnostic| {
            diagnostic.profile == profile
                && diagnostic.path.as_deref() == Some(dataset.metadata.provenance.path.as_str())
                && diagnostic.format_candidate.as_ref() == Some(&dataset.format)
        }),
        "all tolerated recoveries retain structured context"
    );
}

fn assert_strict_and_compatibility_accept(contents: &str, expected: ExpectedTable) {
    let file = temporary_csv(contents);
    let strict = read(file.path()).unwrap();
    assert_table(&strict, &expected, ReadProfile::Strict);
    let compatibility = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
    assert_table(&compatibility, &expected, ReadProfile::Compatibility);
}

fn header_diagnostic(row: usize) -> (DiagnosticCode, RecoveryAction, usize, Option<usize>) {
    (
        DiagnosticCode::RowClassification,
        RecoveryAction::HeaderInterpreted,
        row,
        None,
    )
}

fn headerless_diagnostics(
    row: usize,
) -> Vec<(DiagnosticCode, RecoveryAction, usize, Option<usize>)> {
    vec![
        (
            DiagnosticCode::HeaderlessRoleInference,
            RecoveryAction::RolesInferred,
            row,
            None,
        ),
        (
            DiagnosticCode::UnknownUnit,
            RecoveryAction::Preserved,
            row,
            None,
        ),
    ]
}

#[test]
fn regression_first_numeric_row_with_missing_cell_is_never_silently_dropped() {
    let file = temporary_csv("0,,2\n1,4,5\n2,6,7\n");
    let dataset = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();

    assert_eq!(dataset.canonical_columns().len(), 3);
    assert_eq!(dataset.canonical_columns()[0].values.len(), 3);
    assert_eq!(dataset.source_row(0), Some(1));
    assert_eq!(dataset.canonical_columns()[0].values[0], Some(0.0));
    assert_eq!(dataset.canonical_columns()[1].values[0], None);
    assert_eq!(dataset.canonical_columns()[2].values[0], Some(2.0));
    assert_eq!(dataset.canonical_columns()[0].role, ColumnRole::Time);
    assert_eq!(dataset.canonical_columns()[1].role, ColumnRole::Unknown);
    assert_eq!(dataset.canonical_columns()[2].role, ColumnRole::Unknown);
    assert!(
        dataset
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.row == Some(1))
    );
}

#[test]
fn regression_entirely_null_auxiliary_column_is_retained() {
    let file = temporary_csv("Time/s,Potential/V,Aux\n0,1,\n1,2,\n2,3,\n");
    let dataset = read(file.path()).unwrap();

    assert_eq!(dataset.canonical_columns().len(), 3);
    assert_eq!(dataset.canonical_columns()[0].values.len(), 3);
    assert_eq!(dataset.source_row(0), Some(2));
    assert_eq!(dataset.canonical_columns()[0].values[0], Some(0.0));
    assert_eq!(dataset.canonical_columns()[2].values, [None, None, None]);
    assert_eq!(dataset.canonical_columns()[0].role, ColumnRole::Time);
    assert_eq!(dataset.canonical_columns()[2].role, ColumnRole::Unknown);
    assert_eq!(dataset.missing_value_count(), 3);
    assert_eq!(dataset.diagnostics.len(), 4);
}

#[test]
fn header_present_two_column_and_multichannel_tables_preserve_all_data_rows() {
    assert_strict_and_compatibility_accept(
        "Time/s,Potential/V\n0,10\n1,11\n",
        ExpectedTable {
            row_count: 2,
            first_source_row: 2,
            first_row: vec![Some(0.0), Some(10.0)],
            roles: vec![ColumnRole::Time, ColumnRole::Potential],
            missing_positions: vec![],
            diagnostics: vec![header_diagnostic(1)],
        },
    );
    assert_strict_and_compatibility_accept(
        "Time/s,NH4,pH\n0,10,7.0\n1,11,7.1\n",
        ExpectedTable {
            row_count: 2,
            first_source_row: 2,
            first_row: vec![Some(0.0), Some(10.0), Some(7.0)],
            roles: vec![ColumnRole::Time, ColumnRole::Unknown, ColumnRole::Unknown],
            missing_positions: vec![],
            diagnostics: vec![header_diagnostic(1)],
        },
    );
}

#[test]
fn headerless_shapes_and_numeric_spellings_preserve_the_first_row() {
    let cases = [
        (
            "two-column zero",
            "0,10\n1,11\n",
            vec![Some(0.0), Some(10.0)],
            vec![ColumnRole::Time, ColumnRole::Unknown],
        ),
        (
            "multichannel",
            "0,10,7.0\n1,11,7.1\n",
            vec![Some(0.0), Some(10.0), Some(7.0)],
            vec![ColumnRole::Time, ColumnRole::Unknown, ColumnRole::Unknown],
        ),
        (
            "negative",
            "-1,-10\n0,-9\n",
            vec![Some(-1.0), Some(-10.0)],
            vec![ColumnRole::Time, ColumnRole::Unknown],
        ),
        (
            "scientific notation",
            "1e-3,-2.5E2\n2e-3,-1.5E2\n",
            vec![Some(0.001), Some(-250.0)],
            vec![ColumnRole::Time, ColumnRole::Unknown],
        ),
    ];
    for (name, contents, first_row, roles) in cases {
        let file = temporary_csv(contents);
        let strict = read(file.path()).unwrap_err();
        assert!(
            matches!(strict, Error::AmbiguousHeaderlessSchema { .. }),
            "{name}: {strict:?}"
        );
        assert_eq!(strict.path(), Some(file.path()));
        assert_eq!(strict.format_candidate(), Some(&FormatId::RegularXy));
        assert!(strict.diagnostics().is_empty());

        let dataset = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
        assert_table(
            &dataset,
            &ExpectedTable {
                row_count: 2,
                first_source_row: 1,
                first_row,
                roles,
                missing_positions: vec![],
                diagnostics: headerless_diagnostics(1),
            },
            ReadProfile::Compatibility,
        );
    }

    let one_row = temporary_csv("0,-1\n");
    assert!(matches!(
        read(one_row.path()),
        Err(Error::AmbiguousHeaderlessSchema { .. })
    ));
    let dataset = read_with_options(one_row.path(), &ReadOptions::compatibility()).unwrap();
    assert_table(
        &dataset,
        &ExpectedTable {
            row_count: 1,
            first_source_row: 1,
            first_row: vec![Some(0.0), Some(-1.0)],
            roles: vec![ColumnRole::Time, ColumnRole::Unknown],
            missing_positions: vec![],
            diagnostics: headerless_diagnostics(1),
        },
        ReadProfile::Compatibility,
    );
}

#[test]
fn headerless_first_row_with_missing_cells_uses_explicit_or_compatibility_policy() {
    for contents in ["0,,2\n1,4,5\n2,6,7\n", "0,NA,2\n1,4,5\n2,6,7\n"] {
        let file = temporary_csv(contents);
        assert!(matches!(
            read(file.path()),
            Err(Error::AmbiguousHeaderlessSchema { .. })
        ));
        let mut diagnostics = vec![(
            DiagnosticCode::MissingValue,
            RecoveryAction::Preserved,
            1,
            Some(2),
        )];
        diagnostics.extend(headerless_diagnostics(1));
        let expected = ExpectedTable {
            row_count: 3,
            first_source_row: 1,
            first_row: vec![Some(0.0), None, Some(2.0)],
            roles: vec![ColumnRole::Time, ColumnRole::Unknown, ColumnRole::Unknown],
            missing_positions: vec![(0, 1)],
            diagnostics,
        };
        let compatibility = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
        assert_table(&compatibility, &expected, ReadProfile::Compatibility);

        let explicit = ReadOptions::new().with_header_policy(HeaderPolicy::Absent);
        let dataset = read_with_options(file.path(), &explicit).unwrap();
        assert_table(&dataset, &expected, ReadProfile::Strict);
    }
}

#[test]
fn malformed_first_row_is_data_only_under_an_explicit_policy_and_remains_structured() {
    let file = temporary_csv("0,bad,2\n1,4,5\n");
    let strict_options = ReadOptions::new().with_header_policy(HeaderPolicy::Absent);
    let error = read_with_options(file.path(), &strict_options).unwrap_err();
    assert!(matches!(
        error,
        Error::MalformedNumeric {
            row: 1,
            column: 2,
            ..
        }
    ));
    assert_eq!(error.path(), Some(file.path()));
    assert_eq!(error.format_candidate(), Some(&FormatId::RegularXy));
    assert_eq!(error.row(), Some(1));
    assert_eq!(error.column(), Some(2));

    let compatibility_options =
        ReadOptions::compatibility().with_header_policy(HeaderPolicy::Absent);
    let dataset = read_with_options(file.path(), &compatibility_options).unwrap();
    let mut diagnostics = vec![
        (
            DiagnosticCode::MalformedNumeric,
            RecoveryAction::ValueReplacedWithMissing,
            1,
            Some(2),
        ),
        (
            DiagnosticCode::ValueReplacedWithMissing,
            RecoveryAction::ValueReplacedWithMissing,
            1,
            Some(2),
        ),
    ];
    diagnostics.extend(headerless_diagnostics(1));
    assert_table(
        &dataset,
        &ExpectedTable {
            row_count: 2,
            first_source_row: 1,
            first_row: vec![Some(0.0), None, Some(2.0)],
            roles: vec![ColumnRole::Time, ColumnRole::Unknown, ColumnRole::Unknown],
            missing_positions: vec![(0, 1)],
            diagnostics,
        },
        ReadProfile::Compatibility,
    );
}

#[test]
fn comment_and_empty_preambles_preserve_header_or_first_data_row() {
    let header = temporary_csv("# acquisition,bench,alpha\nTime/s,Potential/V\n0,10\n1,11\n");
    let expected_header = ExpectedTable {
        row_count: 2,
        first_source_row: 3,
        first_row: vec![Some(0.0), Some(10.0)],
        roles: vec![ColumnRole::Time, ColumnRole::Potential],
        missing_positions: vec![],
        diagnostics: vec![
            (
                DiagnosticCode::RowClassification,
                RecoveryAction::MetadataPreserved,
                1,
                None,
            ),
            header_diagnostic(2),
        ],
    };
    let strict = read(header.path()).unwrap();
    assert_table(&strict, &expected_header, ReadProfile::Strict);
    assert_eq!(strict.metadata.raw_rows.len(), 1);
    assert_eq!(strict.metadata.raw_rows[0].row_number, 1);
    let compatibility = read_with_options(header.path(), &ReadOptions::compatibility()).unwrap();
    assert_table(&compatibility, &expected_header, ReadProfile::Compatibility);

    let headerless = temporary_csv("// acquisition bench\n0,10\n1,11\n");
    assert!(matches!(
        read(headerless.path()),
        Err(Error::AmbiguousHeaderlessSchema { .. })
    ));
    let mut expected_diagnostics = vec![(
        DiagnosticCode::RowClassification,
        RecoveryAction::MetadataPreserved,
        1,
        None,
    )];
    expected_diagnostics.extend(headerless_diagnostics(2));
    let dataset = read_with_options(headerless.path(), &ReadOptions::compatibility()).unwrap();
    assert_table(
        &dataset,
        &ExpectedTable {
            row_count: 2,
            first_source_row: 2,
            first_row: vec![Some(0.0), Some(10.0)],
            roles: vec![ColumnRole::Time, ColumnRole::Unknown],
            missing_positions: vec![],
            diagnostics: expected_diagnostics,
        },
        ReadProfile::Compatibility,
    );
    assert_eq!(dataset.metadata.raw_rows.len(), 1);

    for (name, contents, first_source_row, header_present) in [
        (
            "before header",
            "\n\nTime/s,Potential/V\n0,10\n1,11\n",
            4,
            true,
        ),
        ("before data", "\n\n0,10\n1,11\n", 3, false),
    ] {
        let file = temporary_csv(contents);
        let options = if header_present {
            ReadOptions::new()
        } else {
            ReadOptions::compatibility()
        };
        let dataset = read_with_options(file.path(), &options).unwrap();
        let mut diagnostics = vec![
            (
                DiagnosticCode::RowClassification,
                RecoveryAction::EmptyRowIgnored,
                1,
                None,
            ),
            (
                DiagnosticCode::RowClassification,
                RecoveryAction::EmptyRowIgnored,
                2,
                None,
            ),
        ];
        if header_present {
            diagnostics.push(header_diagnostic(3));
        } else {
            diagnostics.extend(headerless_diagnostics(3));
        }
        assert_table(
            &dataset,
            &ExpectedTable {
                row_count: 2,
                first_source_row,
                first_row: vec![Some(0.0), Some(10.0)],
                roles: if header_present {
                    vec![ColumnRole::Time, ColumnRole::Potential]
                } else {
                    vec![ColumnRole::Time, ColumnRole::Unknown]
                },
                missing_positions: vec![],
                diagnostics,
            },
            options.profile,
        );
        assert_eq!(dataset.metadata.raw_rows.len(), 2, "{name}");
    }
}

#[test]
fn ambiguous_mixed_first_row_is_strict_error_or_diagnosed_policy_decision() {
    let file = temporary_csv("Time/s,1,Signal\n0,10,7\n1,11,8\n");
    let error = read(file.path()).unwrap_err();
    assert!(matches!(error, Error::AmbiguousFirstRow { row: 1, .. }));
    assert_eq!(error.category(), ErrorCategory::Schema);
    assert_eq!(error.path(), Some(file.path()));
    assert_eq!(error.format_candidate(), Some(&FormatId::RegularXy));
    assert_eq!(error.row(), Some(1));
    assert_eq!(error.column(), None);
    assert_eq!(error.diagnostics().len(), 1);
    assert_eq!(
        error.diagnostics()[0].code,
        DiagnosticCode::RowClassification
    );
    assert_eq!(error.diagnostics()[0].severity, Severity::Error);
    assert_eq!(error.diagnostics()[0].recovery, RecoveryAction::None);

    let expected = ExpectedTable {
        row_count: 2,
        first_source_row: 2,
        first_row: vec![Some(0.0), Some(10.0), Some(7.0)],
        roles: vec![ColumnRole::Time, ColumnRole::Unknown, ColumnRole::Unknown],
        missing_positions: vec![],
        diagnostics: vec![header_diagnostic(1)],
    };
    let compatibility = read_with_options(file.path(), &ReadOptions::compatibility()).unwrap();
    assert_table(&compatibility, &expected, ReadProfile::Compatibility);
    assert_eq!(compatibility.diagnostics[0].severity, Severity::Warning);

    let explicit = ReadOptions::new().with_header_policy(HeaderPolicy::Present);
    let dataset = read_with_options(file.path(), &explicit).unwrap();
    assert_table(&dataset, &expected, ReadProfile::Strict);
    assert_eq!(dataset.diagnostics[0].severity, Severity::Warning);
}

#[test]
fn entirely_null_auxiliary_column_is_valid_in_strict_and_compatibility_modes() {
    let expected = ExpectedTable {
        row_count: 3,
        first_source_row: 2,
        first_row: vec![Some(0.0), Some(1.0), None],
        roles: vec![ColumnRole::Time, ColumnRole::Potential, ColumnRole::Unknown],
        missing_positions: vec![(0, 2), (1, 2), (2, 2)],
        diagnostics: vec![
            header_diagnostic(1),
            (
                DiagnosticCode::MissingValue,
                RecoveryAction::Preserved,
                2,
                Some(3),
            ),
            (
                DiagnosticCode::MissingValue,
                RecoveryAction::Preserved,
                3,
                Some(3),
            ),
            (
                DiagnosticCode::MissingValue,
                RecoveryAction::Preserved,
                4,
                Some(3),
            ),
        ],
    };
    assert_strict_and_compatibility_accept("Time/s,Potential/V,Aux\n0,1,\n1,2,\n2,3,\n", expected);
}

#[test]
fn duplicate_semantic_roles_retain_path_format_and_diagnostics_in_both_profiles() {
    let file = temporary_csv("Time/s,Time/s,Potential/V\n0,0,1\n1,1,2\n2,2,3\n");
    for options in [ReadOptions::new(), ReadOptions::compatibility()] {
        let error = read_with_options(file.path(), &options).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Accessor);
        assert_eq!(error.path(), Some(file.path()));
        assert_eq!(error.format_candidate(), Some(&FormatId::RegularXy));
        assert_eq!(error.row(), None);
        assert_eq!(error.column(), None);
        assert_eq!(error.diagnostics().len(), 1);
        assert_eq!(
            error.diagnostics()[0].code,
            DiagnosticCode::RowClassification
        );
        assert_eq!(
            error.diagnostics()[0].recovery,
            RecoveryAction::HeaderInterpreted
        );
        assert_eq!(error.diagnostics()[0].profile, options.profile);
        assert_eq!(error.diagnostics()[0].path.as_deref(), file.path().to_str());
    }
}

#[test]
fn regression_duplicate_semantic_role_error_retains_read_context() {
    let file = temporary_csv("Time/s,Time/s,Potential/V\n0,0,1\n1,1,2\n2,2,3\n");
    let options = ReadOptions::new().with_validation(ValidationLevel::Scientific);
    let error = read_with_options(file.path(), &options).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Accessor);
    assert_eq!(error.path(), Some(file.path()));
    assert_eq!(error.format_candidate(), Some(&FormatId::RegularXy));
    assert_eq!(error.row(), None);
    assert_eq!(error.column(), None);
}
