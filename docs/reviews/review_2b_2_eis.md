## P0 blocking

None.

## P1 major

1. Missing frequency does not produce `MissingRequiredRole`.

   EIS detection itself requires a frequency role at [src/formats/chi/mod.rs:227](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs:227), so an otherwise recognizable EIS file without frequency never reaches required-role validation at [src/reader.rs:264](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/reader.rs:264).

   Runtime result: `UnknownFormat`, no format candidate, zero diagnostics. Missing real or imaginary correctly produces `MissingRequiredRole`.

2. Duplicate or ambiguous EIS headers lack actionable structured diagnostics.

   Duplicate roles and unresolved header roles are rejected inside the detection predicate at [src/formats/chi/mod.rs:216](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs:216) and [src/formats/chi/mod.rs:237](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs:237). Runtime probes for duplicate real impedance and an ambiguous impedance header returned generic `UnknownFormat` errors with zero diagnostics and no offending role or column.

   Duplicate assignment is rejected, satisfying the minimum wording of item 14, but item 15 is not satisfied.

::code-comment{title="[P1] Missing frequency bypasses required-role validation" body="The detector requires Frequency before selecting ChiEis, so an EIS-signature input missing frequency returns generic UnknownFormat instead of MissingRequiredRole { role: Frequency }. Allow recognizable incomplete EIS schemas to reach the existing required-role validation." file="/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs" start=227 priority=1}

::code-comment{title="[P1] Header ambiguity loses role diagnostics" body="Duplicate or unresolved EIS header roles are rejected by the detection predicate, leaving callers with UnknownFormat and no role/column diagnostics. Preserve a lower-confidence EIS candidate and report a structured duplicate/ambiguous-role schema error." file="/Users/xingyuwang/ProjectOngoing/electrodata-io/src/formats/chi/mod.rs" start=216 end=244 priority=1}

## P2 moderate

None.

## Verified correct

| Requirement | Result |
|---|---|
| Frequency required | Enforced, but P1 error-contract gap above |
| Real impedance required | Pass |
| Imaginary impedance required | Pass |
| Magnitude optional | Pass |
| Phase optional | Pass |
| Arbitrary column order | Pass |
| Three-column EIS | Pass |
| Four-column EIS | Pass |
| Five-column EIS | Pass |
| Reordered EIS | Pass |
| Semantic-header-only detection, without instrument preamble | Pass at 0.93 versus 1.00 with CHI preamble |
| Measured optional values preserved | Pass |
| Missing optional columns not synthesized in stored schema | Pass; derived only through typed accessors |
| Duplicate roles rejected | Pass, but generically |
| Ambiguous headers structured | Fail; P1 above |
| Stable typed `eis_view()` numerical access | Pass |
| Consumers need not use/import Polars | Pass; `EisView` exposes crate-owned `Column` and numerical values |
| Missing required roles structured | Pass for real/imaginary; fail for frequency |

The core typed implementation is at [src/dataset.rs:228](/Users/xingyuwang/ProjectOngoing/electrodata-io/src/dataset.rs:228). All four EIS fixtures passed, including exact measured-value preservation and three-column derivation behavior.

## Commands executed

All passed:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

Also passed the targeted EIS fixture, typed-view, semantic-header detection, missing-role, and unit/value tests. A read-only external runtime probe confirmed the two P1 behaviors. The repository remained clean; no files were modified.

## Release recommendation for EIS support

No-go for release as fully compliant EIS support until the two P1 error-path gaps are fixed. Valid 3/4/5-column, reordered, and typed-access EIS behavior is release-ready, but the required structured schema diagnostics are incomplete.
