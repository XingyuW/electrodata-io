# Historical Review 2B-3 finding — remediated

This is the pre-remediation review record. Its P1 worksheet-context finding was
addressed by the follow-up patch: resolved XLSX worksheet name and zero-based
index are preserved in `ErrorContext`/`ReadContext` and dataset provenance;
the underlying structured error remains the `ReadContext` source. The focused
XLSX regression suite covers explicit name/index, automatic selection, CHI,
EIS schema, malformed time/value, and typed-view failures. Rerun Review 2B-3
against the remediation commit for a fresh canonical-read verdict.

## P0 blocking

None.

## P1 major

### XLSX parse errors lose worksheet context (resolved)

- **File:** [src/error.rs](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/error.rs:452), [src/error.rs](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/error.rs:537)
- **Symbol:** `Error::worksheet`, `Error::with_read_context`, `Error::with_read_context_and_diagnostics`
- **Exact problem:** Path-bearing errors are returned without a `ReadContext` wrapper. Most parse variants already contain a path but no worksheet field, so the known worksheet is discarded. A read-only probe using worksheet `ocpt` produced:
  ```text
  MalformedNumeric
  path=Some(".../malformed_chi.xlsx")
  worksheet=None
  row=Some(6)
  column=Some(2)
  diagnostics=[]
  ```
- **Consumer consequence:** `rust_electroanalysis_cli` cannot reliably report the worksheet containing an invalid time, invalid cell, ragged row, coordinate-order failure, or similar workbook error. For CHI worksheets there may be no diagnostic from which to recover the sheet name.
- **Minimum safe fix:** Preserve a singular worksheet whenever one is known—either wrap path-bearing errors in `ReadContext` when worksheet context is missing, or add worksheet fields consistently. Ensure typed-accessor errors created from an XLSX `Dataset` also retain `metadata.provenance.worksheet`.
- **Regression test:** Create a multi-sheet XLSX with a compatible CHI time-series worksheet containing a malformed cell after the detection rows. Assert `error.context().worksheet == Some("ocpt")`, alongside path, format, row, and column. Repeat for ragged/missing-coordinate and typed-view errors.

::code-comment{title="[P1] XLSX errors discard the selected worksheet" body="Path-bearing parse errors return early without a ReadContext wrapper, even when a worksheet was supplied. Variants such as MalformedNumeric do not store a worksheet themselves, so ErrorContext reports worksheet=None; CHI failures may also have no diagnostics from which consumers could recover it." file="/Users/xingyuwang/ProjectOngoing/electrodata-io/src/error.rs" start=543 end=544 priority=1}

## P2 moderate

None beyond the P1 context-loss family above.

## P3/minor debt

The documented legacy compatibility surface still exposes Polars through `Dataset::data`, `column_by_role`, `channel`, and `Error::Polars`. Ordinary canonical consumers do not need these APIs, and no Calamine types leak into normal consumer code. This is non-blocking compatibility debt.

## Verified correct

- Arbitrary N-column and legacy two-column numeric tables.
- Multichannel time series, unknown columns, units, null alignment, and source rows.
- Explicit headerless behavior; first numeric rows are retained.
- Strict defaults and opt-in, fully diagnosed compatibility recovery.
- Content-aware DAT dispatch and structured binary rejection.
- Three-, four-, five-column, and reordered EIS.
- Optional measured magnitude/phase and typed derivation only when absent.
- Conservative automatic EIS detection.
- Structured missing/duplicate/ambiguous EIS role failures.
- Automatic XLSX read and detection return `AmbiguousWorksheet` whenever two compatible sheets exist, including unequal-confidence candidates.
- Explicit `SheetSelector::Name` and zero-based `Index` are deterministic.
- XLSX data uses the canonical `Dataset`, preserves nulls, roles, units, provenance, and formula status.
- Canonical read, time-series, and EIS APIs require neither Polars nor Calamine imports.

## Consumer fixtures verified

All 17 entries in `tests/fixtures/rust_electroanalysis_cli/expected.toml` passed the manifest-driven comparison:

- dataset kind, row/column counts;
- roles and units;
- missing counts and focused missing-position checks;
- exact diagnostics;
- strict and compatibility outcomes.

No unexplained mismatch was found.

## Backward compatibility

- **Preserved:** public read functions, `ElectroDataError`, legacy direct readers, `Dataset::data`/`columns`, Polars accessors, `FormatHandler`, and legacy role aliases.
- **Deprecated with migration path:** no formal `#[deprecated]` APIs; README documents canonical replacements for legacy surfaces.
- **Intentionally breaking:** `ReadOptions` struct literals, detailed `Error` destructuring, exhaustive `ColumnRole` matches, and some serde spellings. These are documented as the planned pre-1.0 `0.2.0` transition in README/CHANGELOG.
- No undocumented compatibility break was found.

## Commands executed

All requested validation commands passed:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

`cargo test --all`: **90 passed, 0 failed**.

Focused suites also passed:

```text
cargo test --test xlsx_worksheet_selection -- --nocapture
cargo test --test consumer_contract manifest_drives_every_consumer_fixture -- --nocapture
cargo test --test table_parsing_remediation -- --nocapture
cargo test --test eis_structured_errors -- --nocapture
```

## Commands failed

No required or focused validation command failed. Two incidental environment checks were unavailable: system Python lacked `openpyxl`—the bundled runtime succeeded—and sandboxing denied `ps`.

## Tested commit SHA

```text
696c2ad6af59846cfe91ae5a0c86f3c864b9a643
```

The working tree remained clean; no repository code was modified.

## Migration recommendation

Do not migrate `rust_electroanalysis_cli` yet. Preserve worksheet identity across fatal XLSX parse and typed-view errors, add the regression coverage above, and rerun this review. The prior XLSX ambiguity blocker and the approved EIS behavior are otherwise intact.
