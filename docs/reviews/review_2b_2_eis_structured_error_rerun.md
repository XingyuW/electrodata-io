# Review 2b.2: EIS structured-error rerun

Date: 2026-08-06
Scope: EIS detection, parsing, semantic roles, diagnostics, and typed EIS access only. General table parsing and XLSX behavior were considered only where shared dispatch directly affects EIS.

## Review outcome

Valid EIS ingestion is strong: semantic-role resolution supports arbitrary column order; three-, four-, and five-column EIS sources decode correctly; optional measured magnitude and phase are retained without creating synthetic stored columns; and `eis_view()` exposes typed access without requiring consumers to import Polars.

One P1 gap remains. An explicitly identified two-column EIS table is rejected in detection before EIS schema validation runs. The caller receives an unstructured detection error rather than `MissingRequiredRole` plus an `EisSchema` diagnostic. This fails the structured-error requirement at the minimum-width boundary.

No repository source code was modified during this review.

## Reviewed implementation

- `src/formats/regular_xy.rs`
  - `eis_header_candidates()` resolves normalized scientific header aliases to `Frequency`, `ImpedanceReal`, `ImpedanceImaginary`, `ImpedanceMagnitude`, and `ImpedancePhase`.
  - `eis_schema_candidate()` determines whether semantic header evidence is strong enough to dispatch to the EIS handler.
- `src/formats/chi/mod.rs`
  - `ChiEisHandler::detect()` selects EIS and sets confidence from the preamble and resolved role set.
  - `validate_eis_schema()` rejects ambiguous headers and duplicate roles, then reports each absent required role as `MissingRequiredRole`.
  - `ChiEisHandler::columns()` preserves source order and creates role/unit-bearing descriptors.
- `src/dataset.rs`
  - `Dataset::eis_view()` returns frequency, real, imaginary, and optional measured columns through public canonical `Column` values.
  - `EisView::magnitude_values()` and `phase_values()` return measured values unchanged when available and derive values only when the corresponding source column is absent.
- `src/error.rs` and `src/diagnostics.rs`
  - Structured EIS error and diagnostic payloads retain candidate roles, source location, header text, conflict information, and schema-resolution outcome.
- `tests/consumer_contract.rs` and `tests/automatic_detection.rs`
  - Contract tests cover 3/4/5 columns, reordered columns, header-only confidence, required-role failures, duplicate roles, ambiguous headers, measured optional fields, typed access, and typed unit/value failures.

## Fixtures executed

All EIS fixtures in `tests/fixtures/rust_electroanalysis_cli/` were read through an external temporary Rust consumer that imported only `electrodata_io`:

| Fixture | Result |
|---|---|
| `chi_eis_three_column.csv` | `ImpedanceSpectrum`; frequency `1000`, real `10`, imaginary `-1`; no measured magnitude or phase; accessor-derived magnitude and phase returned expected values. |
| `chi_eis_four_column.csv` | `ImpedanceSpectrum`; frequency `1000`, real `10`, imaginary `-1`; phase remained measured; magnitude was accessor-derived. |
| `chi_eis_five_column.csv` | `ImpedanceSpectrum`; frequency `1000`, real `10`, imaginary `-1`; measured magnitude and phase were retained exactly. |
| `chi_eis_reordered_columns.csv` | `ImpedanceSpectrum`; typed roles returned the same expected frequency, real, and imaginary values despite source order `phase, imaginary, frequency, magnitude, real`. |

The external consumer did not import or directly declare Polars.

## Requirement verification

| # | Requirement | Evidence and result |
|---:|---|---|
| 1 | Frequency is required | `validate_eis_schema()` checks `Frequency`; the missing-frequency contract test returns `MissingRequiredRole(Frequency)`. **Pass.** |
| 2 | Real impedance is required | `validate_eis_schema()` checks `ImpedanceReal`; the missing-real contract test returns `MissingRequiredRole(ImpedanceReal)`. **Pass.** |
| 3 | Imaginary impedance is required | `validate_eis_schema()` checks `ImpedanceImaginary`; the missing-imaginary contract test returns `MissingRequiredRole(ImpedanceImaginary)` for schema candidates. **Pass, subject to P1 at two columns.** |
| 4 | Magnitude is optional | It is excluded from required-role validation and exposed as `Option<&Column>`. **Pass.** |
| 5 | Phase is optional | It is excluded from required-role validation and exposed as `Option<&Column>`. **Pass.** |
| 6 | Column order is arbitrary | Roles are resolved per normalized header, not position. **Pass.** |
| 7 | Three-column EIS works | `chi_eis_three_column.csv` passed. **Pass.** |
| 8 | Four-column EIS works | `chi_eis_four_column.csv` passed; tests also cover magnitude-only and phase-only forms. **Pass.** |
| 9 | Five-column EIS works | `chi_eis_five_column.csv` passed. **Pass.** |
| 10 | Reordered EIS works | `chi_eis_reordered_columns.csv` passed. **Pass.** |
| 11 | Header-only lower confidence | Semantic-header-only EIS detects at `0.93`, below the `1.0` A.C. Impedance preamble fixture. **Pass.** |
| 12 | Measured optional fields preserved | Measured magnitude/phase columns and their null cells remain source-backed values. **Pass.** |
| 13 | Missing optional fields not synthesized | Three-column storage remains three columns. Derived magnitude/phase exist only as `EisView` method results when the measured column is absent; a measured null stays null. **Pass.** |
| 14 | Duplicate role assignment diagnosed | `validate_eis_schema()` rejects duplicates for every EIS role and provides `EisSchemaConflict` details. **Pass.** |
| 15 | Ambiguous headers structured | Multi-role aliases produce `EisSchemaConflict` with candidate roles, source row/column, header, and resolution. **Pass.** |
| 16 | Stable typed `eis_view()` access | Public `EisView` exposes typed canonical `Column` references and null-aware `f64` values. **Pass.** |
| 17 | Consumers do not need Polars | External consumer compiled and read each fixture using only the crate's typed API. **Pass.** |
| 18 | Missing required roles structured | Works for detected schema candidates, but fails for explicit two-column EIS because dispatch never reaches validation. **Partial/fail; P1.** |

## P0 blocking

None.

## P1 major

### Explicit two-column EIS bypasses structured missing-role validation

`ChiEisHandler::detect()` documents that an explicit `A.C. Impedance` signature should let incomplete headers reach `validate_eis_schema()`. Its actual fallback still requires `row.len() >= 3`:

```rust
let (header, data) = eis_header_and_data(probe, |row| {
    eis_schema_candidate(row) || (technique.is_some() && row.len() >= 3)
})?;
```

An explicit source missing one required role has only two EIS columns, for example:

```text
A.C. Impedance

Freq/Hz,Z'/ohm
1000,10
100,15
10,20
```

Observed result from the independent consumer probe:

```text
category=Detection
unknown scientific format ... best confidence 0.000, threshold 0.650
diagnostics=0
```

Expected result:

```text
Error::MissingRequiredRole {
  format_candidate: ChiEis,
  role: ImpedanceImaginary,
  diagnostics: [DiagnosticCode::EisSchema, ...],
  ...
}
```

The structured validation itself is correct once invoked: it checks frequency, real impedance, and imaginary impedance before accepting the schema. The defect is dispatch reachability. The two-column guard prevents an explicitly identified EIS table from obtaining the diagnostic contract promised by the handler comment and required by item 18.

Affected source: `src/formats/chi/mod.rs`, `ChiEisHandler::detect()` fallback predicate (current lines 365-367).

## P2 moderate

None.

## Detailed behavior notes

### Detection and role resolution

- The EIS handler recognizes CHI preambles including `A.C. Impedance`, spacing and period variants, `AC Impedance`, and Electrochemical Impedance Spectroscopy.
- Header-only detection requires semantic evidence for the required complex impedance set and deliberately has lower confidence than a CHI preamble.
- Recognized aliases include frequency (`freq`, `frequency`), real impedance (`Z'`, `Zre`, `real impedance`), imaginary impedance (`Z\"`, `Zim`, `imaginary impedance`), magnitude (`Z`, `|Z|`, `Zmod`, `magnitude`), and phase (`phase`, `phase angle`, `theta`).
- Unknown EIS-adjacent columns remain retained as `Unknown`; they are not assigned a role by position.

### Schema diagnostics

- An ambiguous header such as a combined real-plus-imaginary header returns `EisSchemaConflict`, retaining exact candidates and the resolution `rejected: ambiguous EIS header`.
- Duplicate semantic roles return `EisSchemaConflict`, identifying the duplicate role, source column, and the resolution `rejected: duplicate EIS role`.
- Missing required roles that reach validation return `MissingRequiredRole` with detected roles and an `EisSchema` error diagnostic.

### Typed access and optional fields

- `Dataset::eis_view()` rejects non-EIS data with `InvalidDatasetView`.
- It requires frequency in hertz and complex components in ohms, rejects absent required values with source row/column context, and rejects unknown or incompatible units with typed errors.
- It returns optional measured magnitude in ohms and phase in degrees only when those source columns exist.
- `magnitude_values()` and `phase_values()` never overwrite a measured source value or fill a measured missing cell; derivation happens only when the entire respective measured column is absent.

## Commands executed

Targeted EIS coverage:

```text
cargo test --test consumer_contract eis -- --nocapture
  13 passed, 0 failed

cargo test --test automatic_detection eis -- --nocapture
  3 passed, 0 failed
```

Required repository commands:

```text
cargo fmt --check
  passed

cargo clippy --all-targets --all-features -- -D warnings
  passed

cargo test --all
  73 passed, 0 failed

cargo build --release
  passed
```

The external temporary consumer probe read all four EIS fixtures, called `eis_view()`, verified role-based numeric values and optional-field presence, and reproduced the P1 two-column explicit-EIS failure. Its first invocation could not resolve dependencies inside the sandbox because DNS access to crates.io was restricted; the authorized rerun completed successfully. This did not affect the repository build or test results.

Final repository status before recording this document was clean. This document is the only intended repository change.

## Release recommendation for EIS support

No-go for a release that claims fully compliant EIS structured-error support.

Valid three-, four-, five-column, and reordered EIS imports, semantic role assignment, optional-field retention, diagnostics for detected EIS schemas, and Polars-independent typed access are release-ready. The P1 dispatch gap must be fixed and covered by a regression test before claiming all required-role failures are structured EIS errors.
