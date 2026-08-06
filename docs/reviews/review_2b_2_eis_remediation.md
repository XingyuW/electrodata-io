# Review 2B-2 EIS remediation

This record addresses the independent EIS review in `review_2b_2_eis.md`.
The two reviewed findings are confirmed implementation defects. The reader now
allows an EIS-like header to reach schema validation even when required roles
are missing, duplicated, or ambiguous; it does not alter successful EIS data.

## Confirmed implementation defects

| Severity | File | Function or type | Exact defect | Root cause | Expected behavior | Minimum safe correction | Regression test | Shared table infrastructure affected |
|---|---|---|---|---|---|---|---|---|
| P1 | `src/formats/chi/mod.rs`, `src/reader.rs` | `ChiEisHandler::detect`, `validate_eis_schema` | A header missing `Frequency` was rejected by detection, returning `UnknownFormat` instead of `MissingRequiredRole(Frequency)`. | `eis_roles` required a unique frequency role before returning a CHI EIS candidate. | An EIS-signature table missing any required role reaches required-role validation and returns a contextual structured schema error. | Detect only a minimum two-of-three EIS signature, then validate required roles after format selection. | `eis_missing_frequency_reaches_required_role_validation`; `eis_missing_real_and_duplicate_frequency_are_structured_schema_errors`; existing missing-imaginary test. | Yes: EIS exclusion in regular-table classification and shared semantic header recognition were adjusted; delimiter detection, first-row buffering, row parsing, and missing-cell policy were unchanged. |
| P1 | `src/formats/chi/mod.rs`, `src/error.rs`, `src/diagnostics.rs` | `validate_eis_schema`, `Error::EisSchemaConflict`, `Diagnostic` | Duplicate or multi-role EIS headers were discarded by detection and surfaced as generic format failures with no offending column, header, candidate roles, or decision. | Detection used uniqueness as a predicate instead of retaining schema evidence for the error path. | Duplicate and ambiguous headers fail as `ErrorCategory::Schema`, preserving the one-based header row and column, source header, candidate roles, conflicting role where applicable, and a structured rejection diagnostic. | Preserve candidates through selection and introduce one contextual EIS schema-conflict error plus diagnostic fields for candidates and resolution. | `eis_duplicate_and_ambiguous_headers_have_schema_diagnostics`; duplicate-frequency regression. | Yes: EIS exclusion in regular-table classification and shared semantic header recognition were adjusted; delimiter detection, first-row buffering, row parsing, and missing-cell policy were unchanged. |

## Missing regression coverage

No separate review finding was rejected as coverage-only. The confirmed defects
had insufficient regression coverage, so focused tests were added before their
implementations were changed. The focused suite also covers 3/4/5 columns,
reordering, CHI preamble, header-only detection, mixed case, punctuation,
optional nulls, unrelated numeric columns, non-EIS tables, and headerless
numeric tables.

## Contract ambiguity

None. The canonical contract clearly requires all three complex-impedance
roles, role-based ordering, optional measured magnitude/phase, and structured
diagnostics.

## Documentation defect

`docs/consumer_contracts/rust_electroanalysis_cli.md` still described EIS as
fixed-order/five-column-only even though that behavior had already been
superseded. Its current-behavior paragraph now describes semantic 3/4/5-column
detection and the structured schema-error behavior. No consumer code changed.

## Reviewer misunderstanding

None. Both P1 findings were reproduced by the new failing tests before the
correction. The review's verified-correct observations about optional measured
columns, arbitrary ordering, typed access, and header-only confidence remain
unchanged.

## Shared-reader assessment

Changed: generic semantic-header recognition and the regular-table handler's
decision to defer a sufficiently strong EIS signature to the EIS handler.

Unchanged: delimiter detection, first-row buffering, generic header-presence
classification, numeric row parsing, and missing-cell policy. Because
regular-table classification is shared, the full Review 2B-1 regression suite
must be run and Review 2B-1 should be repeated after this change.
