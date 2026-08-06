# Canonical-read review 01

Date: 2026-08-06
Review baseline: preimplementation commit `7886e60`
Reviewed implementation: dirty working tree that introduced canonical read
Review result: **NO-GO**

This document preserves the independent review findings as the remediation
checklist. The listed fixture and compatibility failures must remain visible;
do not delete, weaken, or hide the tests that expose them.

## Required command results

All four requested release commands completed successfully. Their success does
not override the contract failures documented below.

| Command | Exit | Recorded output |
| --- | --- | --- |
| `cargo fmt --check` | 0 | No output. |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.14s` |
| `cargo test --all` | 0 | 44 tests passed, 0 failed; 0 doctests discovered. |
| `cargo build --release` | 0 | `Finished release profile [optimized] target(s) in 0.12s` |

Additional successful checks:

- `cargo test --all-features`: 44 passed, 0 failed.
- `cargo check --no-default-features`
- `cargo check --no-default-features --features csv`
- `cargo check --no-default-features --features excel`
- `git diff --check 7886e60`

## Preserved failed command output

### Out-of-tree public-API compatibility probe

This probe deliberately compiled representative code written against the
preimplementation public API:

```text
$ cargo check --offline --manifest-path /tmp/electrodata_io_independent_review/Cargo.toml --bin preimplementation_api
error[E0063]: missing fields `coordinate_order_policy`, `invalid_cell_policy`,
`invalid_time_policy` and 2 other fields in initializer of `ReadOptions`
 --> src/bin/preimplementation_api.rs:7:5

error[E0027]: pattern does not mention fields `path`, `format_candidate`,
`diagnostics`, `source`
  --> src/bin/preimplementation_api.rs:39:9
   |
39 |         Error::MalformedNumeric { row, column, value } => {
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0026]: variant `WorkbookOpening` does not have a field named `message`
  --> src/bin/preimplementation_api.rs:42:40

error[E0164]: expected tuple struct or tuple variant, found struct variant
`Error::Calamine`
  --> src/bin/preimplementation_api.rs:45:9

error[E0027]: pattern does not mention fields `path`, `source`
  --> src/bin/preimplementation_api.rs:48:9
   |
48 |         Error::InvalidUtf8 { valid_up_to } => {
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0004]: non-exhaustive patterns: `ColumnRole::ImpedancePhase`,
`ColumnRole::Current`, `ColumnRole::MeasurementChannel(_)` and 1 more
not covered
  --> src/bin/preimplementation_api.rs:22:11

error: could not compile `electrodata-io-independent-review`
(bin "preimplementation_api") due to 7 previous errors
```

### Initial temporary harness dependency attempt

The first out-of-tree harness attempt was not run offline and failed only
because this environment cannot resolve the crate registry. The subsequent
offline harness succeeded and produced the fixture findings below.

```text
$ cargo run --quiet --manifest-path /tmp/electrodata_io_independent_review/Cargo.toml
error: failed to get `calamine` as a dependency of package `electrodata-io`

Caused by:
  failed to load source for dependency `calamine`
  unable to update registry `crates-io`
  download of config.json failed
  [6] Couldn't resolve host name (Could not resolve host: index.crates.io)
```

## P0 blocking defects

None confirmed. No implementation patch was made during the review.

## P1 major defects

### 1. The normative fixture manifest is not validated and does not match output

`tests/consumer_contract.rs` only checks that each filename appears in
`expected.toml`; it hard-codes shape and missing-count expectations instead
of parsing the manifest.

Independent fixture comparison found:

- `regular_headerless.csv` exposes canonical column `time`, while the
  manifest requires `column_1`.
- `regular_ragged_rows.csv` emits an additional `MissingValue` diagnostic
  after `RowPadded`; the manifest lists only `RaggedRow`, `RowPadded`,
  and `ExtraCellsIgnored`.

The manifest is explicitly normative in the consumer contract. Add a
manifest-driven test that checks strict result, compatibility result, kind,
row/column/missing counts, source names, roles, units, and the exact
diagnostic contract for every copied fixture.

### 2. Public API compatibility is broken without a documented migration

The implementation adds public `ReadOptions` fields, changes public
`Error` variant shapes, and adds `ColumnRole` variants. These changes break
struct literals, destructuring patterns, exhaustive matches, and potentially
serialized role values. Equality aliases do not preserve Rust pattern matching
or serde compatibility.

The compatibility section of `CHANGELOG.md` says the legacy surfaces remain
available, but does not document these source-incompatible migrations. Either
restore source compatibility where feasible or document a versioned migration
with before/after code and a semver decision.

### 3. Canonical consumer contract remains incomplete

The new typed model is Polars-independent at the accessor boundary, but it
does not yet provide the full consumer contract:

- no unit-checked time-in-seconds accessor or equivalent;
- a `Time` column with `Unit::Unknown` is accepted by
  `time_series_view()`;
- `EisView` can expose a null real/imaginary component;
- no per-column missing-value counts;
- provenance omits delimiter/encoding, detection evidence, active policies,
  source-column positions, and source identity/digest;
- formula/cache handling does not produce `UnusableFormula`.

Implement the missing typed conversion/provenance APIs and test strict versus
compatibility behavior at that boundary.

### 4. Error context is not uniformly structured

Although `ErrorContext` exists, several variants discard known context.
For example, a missing worksheet returns `ErrorCategory::Worksheet` but
`path() == None`. Ambiguity errors also fail to consistently expose worksheet
identity. Make path, worksheet, selected/detected format, row, and column
available whenever known.

## P2 moderate defects

- Individually selecting recovery policies leaves `ReadProfile::Strict`, so
  the diagnostics are labeled strict despite taking compatibility-style action.
- Lossy UTF-8 decoding emits a diagnostic with `RecoveryAction::None`.
- `DiagnosticSummary` lacks malformed-row and per-column missing counts.
- `UnusableFormula` is declared but never emitted.

## Verified behavior

- N-column time-series parsing is supported; data is not restricted to two
  columns.
- EIS requires frequency, real impedance, and imaginary impedance; magnitude
  and phase are optional.
- Three-, four-, five-, and reordered-column EIS fixtures work.
- DAT text files are recognized and binary content is rejected.
- Strict parsing rejects malformed coordinates/cells, ragged rows, and
  duplicate/decreasing timestamps.
- Compatibility policies recover by skipping rows, nulling invalid
  measurements, padding/truncating ragged rows, and preserving coordinate
  order with diagnostics.
- Typed `TimeSeriesView` and `EisView` do not expose Polars types.
- XLSX runs through the same decoding/schema pipeline.

## Repair checklist

- [ ] Replace filename/string checks with a parsed, manifest-driven fixture
  contract test.
- [ ] Reconcile headerless naming and ragged-row diagnostics with
  `expected.toml`, then update the manifest only with an explicit contract
  decision.
- [ ] Add regression tests for all manifest fields and diagnostic actions.
- [ ] Decide and document the public API/semver migration; add compilation
  coverage for supported legacy usage.
- [ ] Complete typed unit conversion, required EIS component handling,
  per-column counts, and provenance.
- [ ] Make structured error context complete for worksheet/detection/schema
  failures.
- [ ] Correct profile/recovery metadata and implement formula diagnostics.
- [ ] Rerun the four required release commands and the fixture contract test.
