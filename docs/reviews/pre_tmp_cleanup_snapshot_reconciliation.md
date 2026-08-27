# Pre-tmp-cleanup snapshot reconciliation

Date: 2026-08-27

This review reconciles every preserved `audit/pre-tmp-cleanup/*` parentless
snapshot against main at `51da5024c00aa3e419f911e43c72e18b05088702`. Because
the snapshots are parentless, conclusions use tree paths, Git blob IDs, exact
diffs, normalized document comparisons, and behavior tests rather than commit
ancestry. The complete path/size/blob inventory is in
[`pre_tmp_cleanup_snapshot_inventory.md`](pre_tmp_cleanup_snapshot_inventory.md).
The exact snapshot-only and main-only path sets are in
[`pre_tmp_cleanup_snapshot_path_comparison.md`](pre_tmp_cleanup_snapshot_path_comparison.md).
The corresponding path sets against ending `main` (including the remediation
artifacts added here) are in
[`pre_tmp_cleanup_snapshot_final_path_comparison.md`](pre_tmp_cleanup_snapshot_final_path_comparison.md).

## Snapshot disposition matrix

`SUPERSEDED` means the changed snapshot implementation is present in main with
the same contract plus later fixes. `PRESENT_EQUIVALENT` means the exact
functionality/content is already present; formatting-only Markdown changes are
included here. `OBSOLETE` means the snapshot material is intentionally not part
of the current crate contract. No snapshot delta is `MISSING_VALID` or
`UNKNOWN`.

| tag | wrapper commit / tree | source differences and valid unique functionality | test differences | document differences | data differences | current-main equivalent and evidence | disposition | action |
|---|---|---|---|---|---|---|---|---|
| `dangling-0d1086` | `9c563c1` / `0d10861` | Earlier versions of `dataset.rs`, `reader.rs`, `formats/*`, metadata, options, validation, and public façade; no valid behavior absent from main. | Earlier generated/consumer coverage; current suite is broader. | Older consumer contract; no review docs in snapshot. | None. | Current `src/lib.rs`, `src/reader.rs`, `src/formats/{regular_xy,chi}/`, `src/dataset.rs`, `src/metadata.rs`, `src/options.rs`, and `src/validation/`; tests `consumer_contract.rs`, `generated.rs`, `table_parsing_remediation.rs`, `eis_structured_errors.rs`, and canonical fixtures. | `SUPERSEDED` for changed source/tests/docs; exact unchanged detection/optional supplied files are `PRESENT_EQUIVALENT`. | None; preserve tag. |
| `dangling-1303d2` | `13bc4c7` / `1303d2c` | Earlier automatic detection and explicit-reader implementation, with older parsing/error behavior. | Earlier automatic, generated, consumer, and supplied tests. | Older README/CHANGELOG/consumer contract. | None. | Detection and façade remain in `src/detection/mod.rs` and `src/lib.rs`; behavior is covered by `tests/automatic_detection.rs`, `tests/consumer_contract.rs`, and `tests/canonical_formats.rs`. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-1438af` | `7662b3c` / `1438af4` | Earlier EIS schema/role handling and metadata/source handling. | Adds earlier EIS structured/table coverage. | Earlier review/README/CHANGELOG material. | None. | `validate_eis_schema`, `ChiEisHandler`, `Dataset::eis_view`, typed roles/units, and structured errors in current `src/formats/chi/mod.rs`, `src/dataset.rs`, `src/error.rs`; tests `eis_structured_errors.rs`, `consumer_contract.rs`, and canonical EIS test. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-2f6ab4` | `ffd8321` / `2f6ab49` | Earlier canonical model, diagnostics, options, provenance, regular/CHI handlers, and detection. | Earlier table parsing and EIS/XLSX tests. | Earlier review documents and consumer contract. | None. | Current canonical model and recovery policies are in `src/{dataset,schema,options,diagnostics,error,metadata}.rs`; production behavior is tested by `consumer_contract.rs`, `table_parsing_remediation.rs`, `eis_structured_errors.rs`, and XLSX tests. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-5eb21c` | `cbbd84e` / `5eb21c9` | Snapshot-only `src/main.rs` is a hello-world binary, not a valid behavior of the current library crate. | None. | Minimal historical README/Cargo metadata. | Four large `data/*.csv` files. | The current package is the library API in `src/lib.rs`; the four data blobs are byte-identical to the ignored local files by Git blob hash (see Data reconciliation). | `OBSOLETE` for `src/main.rs` and historical data inputs. | Do not copy or resurrect; preserve tag. |
| `dangling-8c4cfc` | `e7c1065` / `8c4fcaf` | Same earlier production line as `0d1086`; no valid behavior absent from main. | Earlier generated/consumer coverage. | `canonical_read_review_01.md` is an earlier review artifact. | None. | Current façade/reader/handlers and canonical tests cover the same reads with later structured-error, provenance, and worksheet fixes. | `SUPERSEDED` for source/tests; review artifact is `OBSOLETE`. | None; preserve tag. |
| `dangling-9889d1` | `223ec1d` / `9889d11` | Later pre-main EIS/XLSX/error/source implementation; no valid behavior absent from main. | Earlier EIS/table/XLSX worksheet coverage. | Earlier EIS/XLSX review docs and README. | None. | Current `src/source/excel.rs`, `src/error.rs`, `src/formats/chi/mod.rs`, `src/dataset.rs`, and tests `xlsx_*`, `eis_structured_errors.rs`, and canonical EIS test include the behavior plus worksheet index/context and compatibility fixes. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-a8c5d1` | `e864ef3` / `a8c5d18` | Earlier automatic detection, explicit readers, regular/CHI handlers, and basic dataset model. | Earlier automatic/generated/supplied tests. | None. | None. | Current detection, explicit façade readers, handlers, and `DataReader`/`FormatHandler` APIs remain in `src/{detection,lib,reader,formats}/`; tests `automatic_detection.rs`, `generated.rs`, and canonical tests. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-c1c976` | `7c4face` / `c1c9768` | Initial library implementation: `DataReader`, `FormatHandler`, regular/CHI parsing, metadata/options/validation; no detection module and no valid behavior absent from main. | Earlier generated/supplied tests. | None. | None. | Current reader/handler façade, canonical schema, structured diagnostics/errors, and provenance supersede the initial implementation; tests `consumer_contract.rs`, `generated.rs`, and canonical tests. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-ca3e88` | `3b55a5f` / `ca3e88b` | Earlier canonical model, automatic detection, CHI/EIS handlers, and structured read path. | Earlier automatic/consumer/generated/table tests. | Earlier review documents and consumer contract. | None. | Current equivalent symbols and tests listed in the symbol reconciliation below. | `SUPERSEDED`. | None; preserve tag. |
| `dangling-dba418` | `9994726` / `dba418f` | Initial library implementation, without automatic detection; no valid behavior absent from main. | Only early generated/supplied tests. | Minimal historical README/CHANGELOG. | Four large `data/*.csv` files. | Current public library and production handlers supersede it; data hashes match the ignored local files and are not crate inputs. | `SUPERSEDED` for source; `OBSOLETE` for historical data. | Do not copy data or source; preserve tag. |
| `dangling-review-blobs` | `926e1de` / `51d3c6c` | No source. | None. | Two recovered review Markdown files differ from current canonical copies only by trailing Markdown hard-break whitespace. | None. | Exact content comparison after removing trailing spaces is identical to `docs/reviews/canonical_read_remediation_01.md` and `docs/reviews/review_2b_2_eis_structured_error_rerun.md`. | `PRESENT_EQUIVALENT`. | None; preserve tag. |

Wrapper commit messages are preservation messages, not source history. The
exact timestamp and subject for every wrapper are:

| tag | commit timestamp | exact commit subject |
|---|---|---|
| `dangling-0d1086` | `2026-08-27T12:03:08-04:00` | `Preserve dangling project snapshot 0d10861d62bd6390d87b300ec200c9bf4ec2f919 for pre-tmp cleanup audit` |
| `dangling-1303d2` | `2026-08-27T12:03:08-04:00` | `Preserve dangling project snapshot 1303d2c44166b5ccfcc328b86ae0a1e828a922d6 for pre-tmp cleanup audit` |
| `dangling-1438af` | `2026-08-27T12:03:08-04:00` | `Preserve dangling project snapshot 1438af446a0c1b34e9d7ecabcf593460fe85e720 for pre-tmp cleanup audit` |
| `dangling-2f6ab4` | `2026-08-27T12:03:08-04:00` | `Preserve dangling project snapshot 2f6ab4991e48f53dd8accb9dbcc8da6aed813034 for pre-tmp cleanup audit` |
| `dangling-5eb21c` | `2026-08-27T12:05:20-04:00` | `Preserve legacy data snapshot locally for pre-tmp cleanup audit` |
| `dangling-8c4cfc` | `2026-08-27T12:06:08-04:00` | `Preserve dangling project snapshot 8c4cfcafaa3a74740fad083d04acdf6ee37281c3 for pre-tmp cleanup audit` |
| `dangling-9889d1` | `2026-08-27T12:06:08-04:00` | `Preserve dangling project snapshot 9889d116915892abbb0671176d123a35ecf25ef1 for pre-tmp cleanup audit` |
| `dangling-a8c5d1` | `2026-08-27T12:06:08-04:00` | `Preserve dangling project snapshot a8c5d185b815bda5b2d6c8dbd90fc9a3950c6f9c for pre-tmp cleanup audit` |
| `dangling-c1c976` | `2026-08-27T12:06:08-04:00` | `Preserve dangling project snapshot c1c9768248d578bb737298bc64387f48eedaf6b3 for pre-tmp cleanup audit` |
| `dangling-ca3e88` | `2026-08-27T12:03:08-04:00` | `Preserve dangling project snapshot ca3e88b20b041129efccb70903c96660e64010bb for pre-tmp cleanup audit` |
| `dangling-dba418` | `2026-08-27T12:06:08-04:00` | `Preserve dangling project snapshot dba418f613eab483061f06bba54490575a634270 for pre-tmp cleanup audit` |
| `dangling-review-blobs` | `2026-08-27T12:03:50-04:00` | `Preserve dangling review blobs for pre-tmp cleanup audit` |

Exact trees, paths, sizes, and blobs are recorded in the inventory file.

## Symbol-level source reconciliation

The following mapping was applied to every snapshot containing `src/`. The
snapshot-specific source lines are represented by the source-version groups in
the matrix; no source delta was left unclassified.

| snapshot source group | snapshot symbols/behavior | current-main symbols | behavior evidence | disposition |
|---|---|---|---|---|
| `dba418`, `c1c976` | Initial `Dataset`, `DataReader`, `DataReaderBuilder`, `SourceData`, `FormatHandler`, regular XY and CHI handlers, basic metadata/options/validation. | `src/dataset.rs:Dataset`, `src/reader.rs:{DataReader,DataReaderBuilder,SourceData,assemble}`, `src/formats/mod.rs:FormatHandler`, `src/formats/{regular_xy,chi}/`, `src/metadata.rs`, `src/options.rs`, `src/validation/mod.rs`. | `tests/generated.rs`, `tests/consumer_contract.rs`, `tests/canonical_formats.rs`; current canonical schema, diagnostics, provenance, and structured errors add the later contract. | `SUPERSEDED` |
| `a8c5d1`, `1303d2` | Automatic format detection plus explicit regular/CHI reader façade, including numbered channels. | `src/detection/{mod,normalize}.rs`, `src/lib.rs:{detect_format,read,read_as,read_regular,read_chi_ocpt,read_chi_multichannel_ocpt,read_chi_eis}`, `src/reader.rs:detect_format`, `src/formats/*`. | `tests/automatic_detection.rs`, `tests/generated.rs`, and the four no-skip tests in `tests/canonical_formats.rs`; current bounded detection and structured errors are safer and more complete. | `SUPERSEDED` |
| `ca3e88`, `2f6ab4` | Canonical typed columns/roles/units, compatibility Polars data, recovery policies, diagnostics, metadata/provenance, and EIS view. | `src/schema.rs:{DatasetKind,ColumnRole,Unit,ColumnDescriptor,Column}`, `src/dataset.rs:{TimeSeriesView,EisView,Dataset::{canonical_column,f64_values,channel,eis_view}}`, `src/options.rs`, `src/diagnostics.rs`, `src/metadata.rs`, `src/error.rs`. | `tests/consumer_contract.rs`, `tests/table_parsing_remediation.rs`, `tests/eis_structured_errors.rs`, `tests/xlsx_*`, and `tests/canonical_formats.rs`. | `SUPERSEDED` |
| `1438af`, `9889d1` | EIS schema validation, semantic real/imaginary roles, optional measured columns, CHI metadata, and Excel context/formula handling. | `src/formats/chi/mod.rs:{validate_eis_schema,ChiEisHandler,eis_header_and_data}`, `src/reader.rs:{MetadataContext,parse_metadata}`, `src/source/excel.rs`, `src/error.rs:{ErrorContext,WorksheetCandidate}`. | `tests/eis_structured_errors.rs`, `tests/consumer_contract.rs`, `tests/xlsx_error_context.rs`, `tests/xlsx_preamble.rs`, `tests/xlsx_worksheet_selection.rs`, and canonical EIS/CHI tests. | `SUPERSEDED` |
| `0d1086`, `8c4cfc` | Late review implementation of regular/CHI reads, detection, metadata, validation, and compatibility APIs. | Same current façade plus `src/reader.rs` source-row/provenance assembly, `src/formats/regular_xy.rs` header classification, `src/source/csv_source.rs` encoding/digest, and current worksheet-index/error-context fields. | `tests/automatic_detection.rs`, `tests/generated.rs`, `tests/consumer_contract.rs`, all remediation tests, and `tests/canonical_formats.rs`. Exact unchanged `src/detection/normalize.rs` and selected test blobs are `PRESENT_EQUIVALENT`; changed production behavior is later main. | `SUPERSEDED` / `PRESENT_EQUIVALENT` per delta |
| all source snapshots | Public `.data` escape hatch, compatibility APIs, explicit handlers, regular table/header handling, CHI OCPT/multichannel/EIS parsing, XLSX worksheet selection, metadata, provenance, structured errors, and automatic detection. | `src/lib.rs`, `src/reader.rs`, `src/formats/mod.rs`, `src/formats/regular_xy.rs`, `src/formats/chi/mod.rs`, `src/source/{csv_source,excel}.rs`, `src/dataset.rs`, `src/error.rs`. | `tests/consumer_contract.rs`, `tests/automatic_detection.rs`, `tests/generated.rs`, `tests/table_parsing_remediation.rs`, `tests/eis_structured_errors.rs`, `tests/xlsx_*`, and `tests/canonical_formats.rs`. | `SUPERSEDED` |

Specific technical rejection: the `dangling-5eb21c` `src/main.rs` only prints
`Hello, world!` and its old manifest describes a different binary package. It
cannot be reintroduced without breaking the current library crate/API. The
legacy raw data are test inputs, not production source or a required package
surface, and are intentionally excluded by `/data/` in `.gitignore`.

## Document and data reconciliation

- Exact document diffs were retained. The two recovered review files are
  whitespace-only variants of current files after trailing-space normalization;
  other snapshot review documents are older review artifacts or superseded
  contract text, not missing implementation requirements.
- For each legacy data path, both snapshot tags containing data and the ignored
  live file have the same Git blob ID:

  | path | `dangling-5eb21c` | `dangling-dba418` | live `git hash-object` |
  |---|---|---|---|
  | `data/EIS.csv` | `46581c794c2b111bfd5e69bf8ee5c0ad0b50d0f7` | same | same |
  | `data/Multichannel_OCPT.csv` | `d6d8ecec9dfe58e1cc9c58df9639dab8766da7c9` | same | same |
  | `data/OCPT.csv` | `8468edf78e44e68cfe23baa7a834906855127ce0` | same | same |
  | `data/regular_file.csv` | `ee66fc49570f2c7912664731e77d106fe9634b0a` | same | same |

No large raw data file was added to main. The committed canonical fixtures
under `tests/fixtures/rust_electroanalysis_cli/` provide small production-path
coverage instead.

## Clean-checkout coverage added in this remediation

`tests/canonical_formats.rs` contains four mandatory tests with no conditional
return: `canonical_regular_fixture_executes_production_reader`,
`canonical_chi_ocpt_fixture_executes_production_reader`,
`canonical_chi_multichannel_fixture_executes_production_reader`, and
`canonical_chi_eis_fixture_executes_production_reader`. They use committed
fixtures and exercise detection plus real reads. Together they cover regular
table/header/scientific-notation parsing, duplicate timestamp preservation,
CHI metadata preambles, OCPT data offsets, noncontiguous channels 1 and 5,
EIS descending frequency, negative imaginary impedance, derived EIS values,
and metadata retention. `Cargo.toml` already includes `tests/**` in packages.

The optional large supplied-file tests remain local validation only and now
identify their inputs as optional local datasets; absence of `data/` cannot
silently skip the mandatory canonical tests.
