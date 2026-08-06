## P0 blocking

None found.

## P1 major

1. Headerless tables can silently lose their first row.

With `HeaderPolicy::Auto`, the first row is considered data only if every cell parses as numeric. An empty cell or missing sentinel therefore makes a headerless row look like a header ([regular_xy.rs](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/regular_xy.rs:36)).

Out-of-tree probe:

```text
0,,2
1,4,5
2,6,7
```

Compatibility mode returned two rows, with `source_row(0) == 2`, interpreted the first row as column names, and emitted no diagnostics. This violates explicit headerless handling, missing-value retention, and the requirement that every recovery be diagnosed.

## P2 moderate

1. Entirely-null auxiliary columns are rejected in both modes.

Structural validation rejects every column whose values are all `None` ([validation/mod.rs](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/validation/mod.rs:40)). Thus a valid time series with populated measurements plus an entirely-missing auxiliary channel cannot be retained as an arbitrary N-column table. Missing cells are correctly preserved when at least one value exists.

2. Duplicate semantic roles lose structured source context.

A table with two `Time` headers is correctly rejected as `ColumnRoleAmbiguous`, but coordinate validation propagates that accessor error directly ([reader.rs](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/reader.rs:369)). The returned error reported:

```text
path=None format=None row=None col=None
```

The path and selected format were known. This is a context-retention gap for invalid schemas.

## Verified correct

- Arbitrary N-column tables work for ordinary populated/partially-null columns.
- Legacy two-column parsing and `read_regular` compatibility behavior work.
- Multichannel time-series tables preserve arbitrary, numbered, conventional, and unknown channels.
- Fully numeric headerless tables require strict explicit permission or compatibility mode and generate inference diagnostics.
- Ordinary missing measurement cells remain `None`, including in strict mode.
- Strict mode rejects malformed coordinates, malformed cells, ragged rows, and invalid coordinate order.
- Compatibility and individual recovery policies are explicit and retained in provenance.
- Invalid timestamps follow `Error` versus `SkipRow`.
- Invalid measurement cells follow `Error` versus `Null`.
- Ragged rows follow `Error` versus padding/trailing-cell truncation.
- Tested recoveries generate structured, source-located diagnostics.
- DAT/text dispatch is content-first rather than extension-only.
- Binary input with a text extension is rejected as `UnsupportedBinary`.
- Time-series typed views expose canonical `Column` values without Polars types.
- Normal parsing errors retain path, format, row, column, category, diagnostics, and parser source where applicable.
- The normative [expected.toml](/Users/xingyuwang/ProjectOngoing/electrodata-io/tests/fixtures/rust_electroanalysis_cli/expected.toml:1) contract test passes exactly.

## Commands executed

All requested commands passed:

```text
cargo fmt --check                                      PASS
cargo clippy --all-targets --all-features -- -D warnings
                                                       PASS
cargo test --all                                       PASS
  54 tests passed, 0 failed
cargo build --release                                  PASS
```

Thirteen focused canonical-table/policy contract tests also passed individually. Two out-of-tree Rust probes confirmed the uncovered header-inference and error-context findings.

`git status --short` and `git diff --check` were clean. No repository files were modified.

## Release recommendation for this review area

**NO-GO** until the P1 silent first-row loss is fixed and regression-tested. The two P2 issues should also be addressed before declaring the canonical table contract complete. No patch has been applied.
