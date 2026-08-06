# Review 2B-1 table-parsing remediation

All three review findings were confirmed by failing regression tests before the
reader was changed.

- P1: `HeaderPolicy::Auto` classified a first row as data only when every cell
  parsed as numeric. A missing cell therefore changed a headerless data row into
  an implicit header, advanced `data_start_row`, and silently lost that row.
- P2: structural validation rejected every entirely-null column, including
  auxiliary measurement columns that are valid aligned source data.
- P2: coordinate validation propagated duplicate-role accessor errors before
  attaching the known path, selected format, and accumulated diagnostics.

## File-level plan and implemented correction

- `src/formats/regular_xy.rs` classifies the first table row as header,
  numeric/missing data, or ambiguous. Auto strict mode rejects ambiguity;
  compatibility or an explicit header policy resolves it visibly. Explicit `#`
  and `//` preambles are excluded from the table and retained as metadata.
- `src/source/csv_source.rs` excludes explicit comment preambles from delimiter
  scoring so comment punctuation cannot select the table delimiter.
- `src/reader.rs`, `src/diagnostics.rs`, and `src/error.rs` record row,
  classification, reason, policy, action, path, format, and profile. Detection
  and decoding continue to use the same replayable parsed-row buffer.
- `src/validation/mod.rs` applies the entirely-null rejection only to required
  coordinate and EIS component roles; arbitrary auxiliary columns remain
  aligned and null-preserving.
- `tests/table_parsing_remediation.rs` covers header-present, headerless,
  one-row, numeric-spelling, missing-cell, preamble, ambiguity, row-preservation,
  and both P2 cases.
- `tests/fixtures/rust_electroanalysis_cli/expected.toml` records the intentional
  new informational header-classification diagnostic. Its data, schema, role,
  missing-value, and recovery expectations are otherwise unchanged.

No input format, EIS-specific behavior, canonical writer, `.edat`, analysis
artifact, or consumer implementation was added or changed.
