## P0 blocking

None.

## P1 major

1. Incomplete, explicitly identified EIS files can bypass structured missing-role validation.

   [`eis_schema_candidate()`](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/regular_xy.rs:267) requires at least three columns and at least two distinct required EIS roles. [`ChiEisHandler::detect()`](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs:338) applies this predicate even when an explicit `A.C. Impedance` technique signature exists.

   Consequently, an explicit EIS file containing only one required role—such as frequency plus magnitude and phase—never reaches [`validate_eis_schema()`](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs:215). The external probe returned:

   ```text
   category=Detection
   diagnostics=0
   unknown scientific format ... best confidence 0.000
   ```

   Expected: `Error::MissingRequiredRole` with `DiagnosticCode::EisSchema`. This means requirement 18 is only satisfied when at least two of frequency/real/imaginary are recognized.

::code-comment{title="[P1] Explicit incomplete EIS bypasses schema diagnostics" body="Detection requires at least three columns and two distinct required EIS roles before selecting ChiEis, even when an A.C. Impedance technique signature is present. Explicit EIS inputs missing two required roles therefore return an unstructured Detection error with no diagnostics instead of reaching validate_eis_schema and returning MissingRequiredRole." file="/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs" start=338 end=340 priority=1}

## P2 moderate

None.

## Verified correct

| Requirement | Result |
|---|---|
| 1–3. Frequency, real, and imaginary required | Pass for accepted EIS candidates |
| 4–5. Magnitude and phase optional | Pass |
| 6. Arbitrary column order | Pass |
| 7. Three-column EIS | Pass |
| 8. Four-column EIS | Pass, including magnitude-only and phase-only coverage |
| 9. Five-column EIS | Pass |
| 10. Reordered EIS | Pass |
| 11. Semantic-header-only detection at lower confidence | Pass: 0.93 versus 1.0 for the CHI preamble fixture |
| 12. Measured optional values preserved | Pass, including measured nulls |
| 13. Missing optional fields not synthesized | Pass in stored columns; magnitude/phase are derived only on typed accessor calls |
| 14. Duplicate roles rejected/diagnosed | Pass for EIS candidates |
| 15. Ambiguous headers produce structured diagnostics | Pass; includes source row/column, header, candidate roles, and resolution |
| 16. Stable typed `eis_view()` access | Pass; exposes frequency, real, imaginary, optional measured columns, and derived accessors |
| 17. Consumers do not need Polars | Pass at the API level; an external consumer compiled without importing or directly declaring Polars. Polars remains an internal transitive crate dependency |
| 18. Missing required roles structured | Partial/fail: pass when exactly one role is missing from an otherwise recognizable schema; P1 above when fewer than two required roles are present |

The typed implementation is centered in [`EisView`](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/dataset.rs:44) and [`Dataset::eis_view()`](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/dataset.rs:229).

Every EIS fixture passed:

- `chi_eis_three_column.csv`
- `chi_eis_four_column.csv`
- `chi_eis_five_column.csv`
- `chi_eis_reordered_columns.csv`

## Commands executed

All succeeded:

```text
cargo test --test consumer_contract eis -- --nocapture
  10 passed

cargo test --test automatic_detection eis -- --nocapture
  3 passed

cargo fmt --check
  passed

cargo clippy --all-targets --all-features -- -D warnings
  passed

cargo test --all
  70 passed, 0 failed

cargo build --release
  passed
```

An external `/private/tmp` consumer probe also read all four EIS fixtures, exercised `eis_view()` without direct Polars use, and reproduced the P1 boundary case.

Repository worktree remained clean; no code was modified.

## Release recommendation for EIS support

No-go for claiming fully compliant EIS support until the P1 structured-error gap is fixed. Valid 3/4/5-column, reordered, optional-field, and typed-access behavior is otherwise release-ready.
