# Canonical-read remediation 01

Date: 2026-08-06
Review source: `docs/reviews/canonical_read_review_01.md`
Baseline: dirty canonical-read working tree over `7886e60`

## Initial validation

All required pre-change commands passed. This does not resolve the independent
contract failures.

| Command | Exit | Result |
| --- | ---: | --- |
| `cargo fmt --check` | 0 | no output |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | passed |
| `cargo test --all` | 0 | 44 tests passed |
| `cargo build --release` | 0 | passed |

## Remediation table

| ID | Severity | Classification | File | Function or type | Exact defect | Expected behavior | Regression test |
| --- | --- | --- | --- | --- | --- | --- | --- |
| R1a | P1 | incorrect test | `tests/consumer_contract.rs` | fixture contract tests | The test searches for filenames and hard-codes selected shapes instead of parsing every normative `expected.toml` field. | Parse the manifest and validate strict/compatibility result, kind, shape, missing counts, source names, roles, units, and exact diagnostic tuples for every fixture. | `manifest_drives_every_consumer_fixture` |
| R1b | P1 | implementation defect | `src/formats/regular_xy.rs` | `RegularXyHandler::columns` | Headerless column 1 is exposed as canonical `time`, contrary to the normative stable name `column_1`. | Preserve generated headerless names `column_1..N` while assigning `Time` only as the inferred role. | Manifest case `regular_headerless.csv` |
| R1c | P1 | implementation defect | `src/reader.rs` | `numeric_frame` | A short row emits `RowPadded` and then a second `MissingValue` diagnostic for the same synthetic padded cell. | Emit the manifest-specified ragged-row and recovery diagnostics exactly; count the padded null without treating it as a source missing token. | Manifest case `regular_ragged_rows.csv` |
| R2 | P1 | documentation-only finding | `CHANGELOG.md`, `README.md` | public migration documentation | Added `ReadOptions` fields, changed `Error` shapes, and new `ColumnRole` variants are source/serde incompatible, but the compatibility note does not say so or make a semver decision. | Declare the next release as a breaking `0.2.0`, document builder/accessor migration with before/after examples, retain safe legacy runtime entry points and aliases, and add compilation coverage for supported legacy surfaces. | `supported_legacy_surfaces_still_compile_and_run` plus doctests/build |
| R3a | P1 | incomplete contract requirement | `src/dataset.rs`, `src/schema.rs`, `src/error.rs` | `Dataset::time_seconds`, `Dataset::time_series_view` | No checked conversion to seconds exists and unknown/incompatible time units pass through the typed time-series boundary. | Convert supported time units explicitly to seconds and return typed unit errors for unknown/incompatible units; expose conversion provenance. | `typed_time_access_is_unit_checked_and_converted` |
| R3b | P1 | incomplete contract requirement | `src/dataset.rs`, `src/error.rs` | `Dataset::eis_view` | Required frequency/real/imaginary columns can contain nulls, and contract units are not checked. | Reject null required EIS components with source location and reject unknown/incompatible required units. | `eis_view_rejects_missing_required_values_and_units` |
| R3c | P1 | incomplete contract requirement | `src/dataset.rs`, `src/diagnostics.rs` | missing-count accessors and `DiagnosticSummary` | Only a total missing count is available; malformed-row and per-column missing counts are absent. | Expose stable per-column missing counts and distinct malformed-row counts. | `summary_reports_malformed_rows_and_per_column_missing_counts` |
| R3d | P1 | incomplete contract requirement | `src/metadata.rs`, `src/reader.rs`, source readers | `Provenance` | Provenance omits delimiter/encoding, detection evidence/confidence, active options/policies, source column positions, and a content digest. | Retain all listed fields as value-semantic metadata for both text and workbooks. | `provenance_retains_detection_policy_columns_and_digest` and manifest fixture checks |
| R3e | P1 | incomplete contract requirement | `src/source/excel.rs`, `src/reader.rs`, `src/error.rs` | workbook formula decoding | Formula presence/cache usability is not inspected and `UnusableFormula` is never emitted. | Accept usable numeric cached results; strict mode returns a typed formula error for unusable caches; compatibility nulls/skips under the matching cell policy and emits `UnusableFormula`. | `xlsx_unusable_formula_is_typed_or_diagnosed` |
| R4a | P1 | implementation defect | `src/error.rs`, `src/source/excel.rs`, `src/detection/mod.rs`, `src/reader.rs` | `ErrorContext`, worksheet/detection variants | Missing-sheet and ambiguity errors discard a known path, and `ErrorContext` has no worksheet identity. | Preserve path and worksheet/candidate identity whenever known while keeping source errors and diagnostics structured. | `worksheet_and_detection_errors_retain_context` |
| R4b | P1 | incomplete contract requirement | `src/formats/chi/mod.rs`, `src/reader.rs`, `src/error.rs` | EIS role validation | An EIS-like header missing a required role falls through to a generic/detection error rather than a structured missing-role error. | Detect the EIS candidate at lower confidence and report the exact missing required role with path/format context. | `eis_missing_required_role_is_structured` |
| R5 | P2 | implementation defect | `src/options.rs`, diagnostic construction | effective read profile | Selecting an individual recovery policy leaves diagnostics labelled `Strict`. | Record `Compatibility` whenever any recovery policy is active and retain the exact individual policies in provenance. | `individual_recovery_policy_updates_diagnostic_profile` |
| R6 | P2 | implementation defect | `src/source/csv_source.rs` | lossy UTF-8 recovery | `LossyDecoding` uses `RecoveryAction::None` even though replacement decoding occurred. | Record a stable lossy-decoding recovery action. | `lossy_utf8_has_explicit_recovery_action` |
| R7 | P1 | incomplete contract requirement | `src/reader.rs`, `src/error.rs`, `README.md` | container dispatch | OLE `.xls` is routed to Calamine even though the consumer contract requires a typed unsupported-container result. | Reject legacy XLS content with path-bearing `UnsupportedContainer`; keep `.xlsx` on the canonical pipeline and content-recognized `.dat` text accepted. | `legacy_xls_is_explicitly_unsupported` |

## Finding separation

- Implementation defects: R1b, R1c, R4a, R5, R6.
- Incorrect tests: R1a.
- Incomplete contract requirements: R3a-R3e, R4b, R7.
- Documentation-only findings: R2.
- Reviewer misunderstandings: none confirmed.

The review correctly distinguishes derived EIS view values from stored source
columns. The remediation must not synthesize magnitude or phase in the reader's
physical column set.
