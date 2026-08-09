# electrodata-io

`electrodata-io` is the canonical input and physical-data representation crate
for electrochemical and scientific tables. Its primary API is a stable,
null-aware `Dataset`/`Column` model; callers do not need Polars. Storage
detection (delimited text or Excel) is independent from scientific-format
detection, and new scientific formats remain registerable through object-safe
handlers.

The crate preserves source order, duplicate samples, numeric signs, raw
metadata, and original headers. It never sorts, deduplicates, resamples,
interpolates, corrects units, or replaces measured values with derived values.

## Table of contents

- [Project structure](#project-structure)
  - [Repository map](#repository-map)
  - [Crates, features, and dependencies](#crates-features-and-dependencies)
  - [Runtime module dependency map](#runtime-module-dependency-map)
  - [Public API and usage map](#public-api-and-usage-map)
  - [Tests, fixtures, and docs](#tests-fixtures-and-docs)
- [Basic use](#basic-use)
  - [Time coordinates and units](#time-coordinates-and-units)
- [Options](#options)
  - [0.2 migration](#02-migration)
  - [XLSX preambles and table headers](#xlsx-preambles-and-table-headers)
  - [XLSX error context](#xlsx-error-context)
- [Registering a format](#registering-a-format)
- [Features](#features)
- [Dependency](#dependency)
- [Extended usage guide](#extended-usage-guide)
  - [Requirements and dependency setup](#requirements-and-dependency-setup)
  - [Reading and reusing a reader](#reading-and-reusing-a-reader)
  - [Format-specific usage](#format-specific-usage)
  - [Configuration patterns](#configuration-patterns)
  - [Direct Polars and metadata access](#direct-polars-and-metadata-access)
- [Validation and diagnostics](#validation-and-diagnostics)
- [Excel worksheet selection](#excel-worksheet-selection)
- [Feature selection](#feature-selection)
- [Deploy or publish an application](#deploy-or-publish-an-application)
- [Publish this crate](#publish-this-crate)
- [Error handling](#error-handling)
  - [Structured EIS failures](#structured-eis-failures)
- [Development and verification](#development-and-verification)

## Project structure

This repository contains one library crate, `electrodata-io`, not a Cargo
workspace. `src/lib.rs` is the public façade; the rest of `src/` is internal
plumbing that is re-exported or consumed through that file.

### Repository map

```text
electrodata-io/
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── src/
│   ├── lib.rs               # public façade and re-exports
│   ├── reader.rs            # read orchestration, decoding, workbook/CSV routing
│   ├── dataset.rs           # canonical Dataset model and typed accessors
│   ├── schema.rs            # roles, units, descriptors, schema helpers
│   ├── options.rs           # read policies and validation configuration
│   ├── metadata.rs          # acquisition, provenance, and raw metadata
│   ├── diagnostics.rs       # non-fatal diagnostics and summaries
│   ├── error.rs             # structured error model and context accessors
│   ├── detection/
│   │   ├── mod.rs           # detection results and selection thresholds
│   │   └── normalize.rs     # text normalization used for matching
│   ├── formats/
│   │   ├── mod.rs           # format IDs, handler trait, shared helpers
│   │   ├── regular_xy.rs    # generic table handler
│   │   └── chi/
│   │       └── mod.rs       # CHI OCPT, multichannel OCPT, and EIS handlers
│   ├── source/
│   │   ├── mod.rs           # container formats and bounded probe type
│   │   ├── csv_source.rs    # CSV/text reading and probe generation
│   │   └── excel.rs         # workbook probing, sheet selection, formula capture
│   └── validation/
│       └── mod.rs           # structural and scientific validation
├── tests/
│   ├── automatic_detection.rs
│   ├── consumer_contract.rs
│   ├── eis_structured_errors.rs
│   ├── generated.rs
│   ├── supplied_files.rs
│   ├── table_parsing_remediation.rs
│   ├── xlsx_error_context.rs
│   ├── xlsx_preamble.rs
│   ├── xlsx_worksheet_selection.rs
│   └── fixtures/rust_electroanalysis_cli/
├── docs/
│   ├── consumer_contracts/
│   └── reviews/
└── LICENSE-MIT / LICENSE-APACHE
```

### Crates, features, and dependencies

| Item | Purpose |
| --- | --- |
| `calamine` | workbook reader backend for `.xlsx`, `.xlsb`, and `.ods` containers; used for sheet enumeration, cell decoding, and formula capture |
| `chrono` | parses CHI acquisition timestamps into `NaiveDateTime` and serializes them when `serde` is enabled |
| `csv` | reads delimited text sources and decodes CSV/TSV records |
| `polars` | provides the compatibility `DataFrame` and `Column` surfaces exposed by `Dataset::data` and `Dataset::column_by_role` |
| `sha2` | computes the SHA-256 provenance digest for the physical source bytes |
| `thiserror` | derives the structured `Error` type and stable display strings |
| `serde` feature | serializes the public metadata, schema, diagnostics, and option types, and enables `chrono/serde` for retained timestamps |
| `csv` feature | enables the `csv` crate and Polars CSV support for delimited text sources |
| `excel` feature | enables workbook input through `calamine` |
| `scientific-validation` feature | enables the optional non-mutating validation rules in `src/validation/mod.rs` |
| `rust_xlsxwriter` (dev) | creates workbook fixtures for the integration tests |
| `tempfile` (dev) | creates isolated temporary CSV/XLSX files in tests |
| `toml` (dev) | reads the consumer-contract manifest fixture |

### Runtime module dependency map

| File / module | Depends on | Purpose and key items |
| --- | --- | --- |
| `src/lib.rs` | `reader`, `dataset`, `detection`, `diagnostics`, `error`, `formats`, `metadata`, `options`, `schema`, `source`, `validation` | public façade; re-exports the core public types and top-level helpers: `read`, `read_with_options`, `read_as`, `read_regular`, `read_chi_ocpt`, `read_chi_multichannel_ocpt`, `read_chi_eis`, and `detect_format` |
| `src/reader.rs` | `source::*`, `formats::*`, `metadata::*`, `validation`, `dataset`, `diagnostics`, `error`, `options`, `schema` | `DataReader`, `DataReaderBuilder`, `SourceData`, `FormulaCell`; container dispatch, preview-driven detection, row decoding, metadata assembly, and validation orchestration |
| `src/dataset.rs` | `schema`, `metadata`, `diagnostics`, `error`, `polars` | `Dataset`, `TimeSeriesView`, `EisView`; canonical column storage, typed accessors, unit conversion, role lookup, source-row tracking, and summary helpers |
| `src/schema.rs` | standard library only plus optional `serde` | `DatasetKind`, `ColumnRole`, `Unit`, `ColumnDescriptor`, `DatasetSchema`, `Column`, `UnitConversion`, `ConvertedValues` |
| `src/options.rs` | `formats::FormatId` plus optional `serde` | `ReadOptions`, `ReadProfile`, `HeaderPolicy`, `SheetSelector`, validation levels, and recovery policies |
| `src/metadata.rs` | `source::ContainerFormat`, `schema::{ColumnRole, Unit}`, `options::ReadOptions`, `FormatId`, `chrono` | `Metadata`, `Provenance`, `AcquisitionMetadata`, `Parameter`, `RawMetadataRow`, `FormulaProvenance`, and source/workbook provenance capture |
| `src/diagnostics.rs` | `schema::{ColumnRole, FormatId, ReadProfile}` plus optional `serde` | `Diagnostic`, `DiagnosticCode`, `Severity`, `RecoveryAction`, `DiagnosticSummary`, `ColumnMissingCount` |
| `src/error.rs` | `formats::FormatId`, `diagnostics::Diagnostic`, `schema::DatasetKind`, `detection::DetectionResult`, `polars`, `calamine`, `thiserror` | `Error`, `ErrorCategory`, `ErrorContext`, `EisSchemaError`, `UnitErrorContext`, `WorksheetCandidate`; stable error inspection, read-context propagation, and workbook-specific failures |
| `src/detection/mod.rs` | `formats`, `error`, `options`, `Path` | `ElectroDataFormat`, `DetectionResult`, `select`; format-level detection thresholds, confidence ordering, and ambiguity handling |
| `src/detection/normalize.rs` | none internal | `normalize_for_match`; text normalization used only for detector matching |
| `src/formats/mod.rs` | `chi`, `regular_xy`, `source::Probe`, `options::ReadOptions`, `schema::ColumnDescriptor` | `FormatId`, `DetectedLayout`, `Detection`, `FormatHandler`, `trim_cell`, `numeric` |
| `src/formats/regular_xy.rs` | `detection::normalize_for_match`, `source::Probe`, `options`, `schema`, `HashMap` | `RegularXyHandler`, `FirstRowClassification`, `classify_first_row`, `header_role`, `eis_header_candidates`, `eis_schema_candidate`, `header_unit` |
| `src/formats/chi/mod.rs` | `regular_xy` helpers, `detection::normalize_for_match`, `source::Probe`, `error`, `diagnostics`, `schema`, `options` | `ChiOcptHandler`, `ChiMultichannelOcptHandler`, `ChiEisHandler`, `validate_eis_schema`; CHI metadata parsing and EIS schema validation |
| `src/source/mod.rs` | `sha2`, `csv_source`, `excel` | `ContainerFormat`, `Probe`, `sha256_hex` |
| `src/source/csv_source.rs` | `source::{ContainerFormat, Probe}`, `diagnostics`, `error`, `csv` | `CsvInput`, `open`, `probe`; UTF-8 handling, delimiter detection, and line parsing |
| `src/source/excel.rs` | `reader::{DataReader, FormulaCell, SourceData}`, `source::Probe`, `calamine`, `error` | `detect`, `read`, `SheetData`, `CompatibleWorksheet`, `resolve_worksheet_candidates`, `formula_cells`, `cell_text` |
| `src/validation/mod.rs` | `dataset`, `diagnostics`, `options`, `error` | `validate`; structural checks, coordinate-order checks, and optional scientific validation |

### Public API and usage map

| API surface | Typical use |
| --- | --- |
| `detect_format(path)` | inspect the built-in format chosen from a bounded preview without loading the whole table |
| `read(path)` | one-off read with automatic detection and default strict options |
| `read_with_options(path, &ReadOptions)` | explicit header, sheet, validation, and recovery policy control |
| `read_as(path, ElectroDataFormat)` | force a built-in scientific format without falling back to another one |
| `read_regular`, `read_chi_ocpt`, `read_chi_multichannel_ocpt`, `read_chi_eis` | deterministic compatibility helpers for the built-in handlers |
| `DataReader::builder()`, `with_builtin_formats()`, `register()`, `build()`, `read()`, `read_with_options()`, `detect_format()` | compose a reusable handler registry and reuse it for repeated reads, explicit options, or preview-only detection |
| `Dataset` | canonical in-memory table plus compatibility `DataFrame` access; key methods include `kind`, `canonical_columns`, `canonical_column`, `f64_values`, `time_seconds`, `time_series_view`, `eis_view`, and `diagnostic_summary` |
| `TimeSeriesView` / `EisView` | narrow the dataset to time-series or impedance-specific access patterns; use `coordinate_values`, `coordinate_unit`, `normalized_seconds`, `magnitude_values`, and `phase_values` |
| `ReadOptions` | configure header policy, sheet selection, validation depth, and recovery behavior; builders include `new`, `compatibility`, `with_profile`, `with_format`, `with_header_policy`, `with_column_name_policy`, `with_sheet`, `with_validation`, and the recovery-policy setters |
| `FormatHandler`, `FormatId`, `Detection`, `DetectedLayout` | register or inspect scientific-format implementations through `format_id`, `detect`, and `columns`, with structured evidence and layout metadata |
| `Error`, `ErrorContext`, `Diagnostic`, `Provenance` | inspect structured failures, worksheet context, and retained source metadata through accessors such as `context`, `source_error`, `category`, `path`, `worksheet`, `row`, `column`, and `diagnostics` |
| `ColumnRole`, `Unit`, `DatasetKind`, `ColumnDescriptor` | describe the semantic schema and units of each source column |

### Tests, fixtures, and docs

| Path | What it covers |
| --- | --- |
| `tests/automatic_detection.rs` | automatic detection, explicit readers, and equivalence between automatic and forced format selection |
| `tests/consumer_contract.rs` | repository-level contract fixtures for the downstream consumer boundary |
| `tests/eis_structured_errors.rs` | EIS schema failures, missing roles, and structured diagnostic context |
| `tests/generated.rs` | generated regression coverage for row preservation, metadata, and compatibility behavior |
| `tests/supplied_files.rs` | checked-in sample files and their preserved metadata/rows |
| `tests/table_parsing_remediation.rs` | header inference, ragged rows, malformed values, and recovery diagnostics |
| `tests/xlsx_error_context.rs` | workbook error propagation, sheet identity, and row/column context |
| `tests/xlsx_preamble.rs` | workbook preambles and semantic header discovery |
| `tests/xlsx_worksheet_selection.rs` | workbook ambiguity handling and explicit worksheet selection |
| `tests/fixtures/rust_electroanalysis_cli/` | fixture CSV/XLSX/DAT inputs plus `expected.toml`, which drives the consumer contract |
| `docs/consumer_contracts/rust_electroanalysis_cli.md` | the downstream boundary contract and semantic expectations |
| `docs/reviews/` | historical review and remediation notes; not part of the runtime dependency graph |

## Basic use

```rust,no_run
use electrodata_io::read;

let dataset = read("experiment.csv")?;
println!("kind={:?}", dataset.kind());
println!("{} rows", dataset.f64_values(&ColumnRole::Time)?.len());
# Ok::<(), electrodata_io::Error>(())
```

`read` performs bounded, content-based automatic detection before dispatching
to the direct built-in reader. It does not identify a scientific format from a
filename. Inspect the decision without loading the full CSV table:

```rust,no_run
use electrodata_io::{detect_format, read};

let detection = detect_format("measurement.csv")?;
println!("Detected format: {:?} ({:.0}% confidence)", detection.format, detection.confidence * 100.0);
let dataset = read("measurement.csv")?;
println!("{:?}", dataset.schema);
# Ok::<(), electrodata_io::Error>(())
```

Use explicit selection when a calling application already knows the instrument
format, has a controlled import workflow, or needs to bypass ambiguity checks:

```rust,no_run
use electrodata_io::{read_as, ElectroDataFormat};

let dataset = read_as("measurement.csv", ElectroDataFormat::ChiEis)?;
# Ok::<(), electrodata_io::Error>(())
```

Canonical columns retain their source names, roles, units, and null-aware
physical values:

```rust,no_run
use electrodata_io::{ColumnRole, read};

let dataset = read("experiment.csv")?;
let view = dataset.time_series_view()?;
println!("{} samples, {} measurements", view.coordinate_values().len(), view.measurements.len());
println!("source coordinate: {:?}", view.coordinate_values());
println!("normalized seconds: {:?}", view.normalized_seconds());
# Ok::<(), electrodata_io::Error>(())
```

### Time coordinates and units

The canonical model keeps three distinct concepts separate:

- **Raw coordinate values** are the original decoded numbers and are available
  through `Dataset::coordinate_values()` or `TimeSeriesView::coordinate_values()`.
- A **physical time coordinate** exists only when the source establishes a
  recognized time unit; inspect it with `coordinate_unit()`.
- **Normalized seconds** are available from `TimeSeriesView::normalized_seconds()`
  only for recognized time units. `Dataset::time_seconds()` performs the same
  conversion but returns a structured unit error when it is unavailable.

For headerless tables, the first numeric column may be inferred as the `Time`
role while its source unit remains `Unit::Unknown`. Its numeric coordinates are
still accessible; they are never silently treated as seconds.

`Dataset::data` and `Dataset::column_by_role` remain available as Polars
compatibility/advanced escape hatches, but are not required by ordinary callers.

Typed and raw metadata are both retained:

```rust,no_run
let dataset = electrodata_io::read("OCPT.csv")?;
println!("{:?}", dataset.metadata.acquisition.recorded_at);
for row in &dataset.metadata.raw_rows {
    println!("{}: {}", row.row_number, row.reconstructed_text);
}
# Ok::<(), electrodata_io::Error>(())
```

Semantic accessors are independent of displayed column names:

```rust,no_run
use electrodata_io::ColumnRole;
let dataset = electrodata_io::read("Multichannel_OCPT.csv")?;
let time = dataset.f64_values(&ColumnRole::Time)?;
let channel_five = dataset.f64_values(&ColumnRole::MeasurementChannel(5))?;
# Ok::<(), electrodata_io::Error>(())
```

EIS roles preserve the recorded imaginary sign and measured values:

```rust,no_run
use electrodata_io::ColumnRole;
let dataset = electrodata_io::read("EIS.csv")?;
let view = dataset.eis_view()?;
let frequency = &view.frequency.values;
let z_imaginary = &view.imaginary.values;
let magnitude = view.magnitude_values(); // derived only when the source column is absent
# Ok::<(), electrodata_io::Error>(())
```

## Options

`ReadOptions::new()` is strict. Recovery is explicit and every tolerated issue
is represented by a source-located diagnostic:

```rust,no_run
use electrodata_io::{
    InvalidCellPolicy, InvalidTimePolicy, RaggedRowPolicy, ReadOptions,
    read_with_options,
};

let options = ReadOptions::new()
    .with_invalid_time_policy(InvalidTimePolicy::SkipRow)
    .with_invalid_cell_policy(InvalidCellPolicy::Null)
    .with_ragged_row_policy(RaggedRowPolicy::PadNulls);
let dataset = read_with_options("legacy.dat", &options)?;
for diagnostic in &dataset.diagnostics {
    println!("{:?} row={:?}: {}", diagnostic.code, diagnostic.row, diagnostic.message);
}
# Ok::<(), electrodata_io::Error>(())
```

`ReadOptions::compatibility()` selects the complete compatibility profile,
including preservation of duplicate/nonmonotonic coordinates. No recovery
sorts, deduplicates, interpolates, changes units, or replaces measured EIS
fields.

### 0.2 migration

The canonical-read API is planned for `0.2.0`, a breaking minor release in the
pre-1.0 series. Construct options through builders instead of a public struct
literal:

```rust
use electrodata_io::{InvalidCellPolicy, ReadOptions};

let options = ReadOptions::new()
    .with_invalid_cell_policy(InvalidCellPolicy::Null);
```

Inspect errors through their stable structured accessors instead of relying on
the field shape of a particular variant:

```rust,no_run
let error = electrodata_io::read("measurement.csv").unwrap_err();
let context = error.context();
eprintln!("{:?} {:?}:{:?}", context.category, context.row, context.column);
```

`Error`, `ReadOptions`, and `ColumnRole` are non-exhaustive for downstream
matching/construction. Safe legacy runtime surfaces remain: the direct built-in
readers, `Dataset::data`, `Dataset::columns`, `ColumnRole::Phase`, and
`ColumnRole::PotentialChannel(n)`. Alias variants retain equality semantics but
not identical Rust pattern or serde representations.

Legacy OLE `.xls` workbooks return a structured `UnsupportedContainer` error.
Use `.xlsx` for workbook input; text `.dat` files continue to be recognized by
content rather than extension alone.

Select an Excel worksheet:

```rust,no_run
use electrodata_io::{ReadOptions, SheetSelector, read_with_options};
let options = ReadOptions::new().with_sheet(SheetSelector::Name("Experiment 2".into()));
let dataset = read_with_options("workbook.xlsx", &options)?;
# Ok::<(), electrodata_io::Error>(())
```

### XLSX preambles and table headers

For workbook sheets, the reader examines a bounded preview for a semantic time
header followed by consistently shaped numeric rows. This permits acquisition
notes and blank rows before a normal table without relying on a fixed number of
rows to skip. The rows before the discovered header are retained in
`metadata.raw_rows`, and dataset row provenance remains the original worksheet
row number. A sheet with no compatible semantic table header is rejected rather
than guessed.

### XLSX error context

Worksheet selection failures (missing, ambiguous, or no compatible worksheet)
occur before a singular worksheet has been resolved. Once an XLSX worksheet is
resolved, every later content/parsing/validation failure is returned through
the public `Error` API with the workbook path, worksheet name, and zero-based
worksheet index where available. The original structured parser error remains
the error source; callers can inspect `error.context()`, `error.worksheet()`,
`error.worksheet_index()`, row/column accessors, and `error.source_error()`
without decoding display text or diagnostics. Typed-view failures after
`Dataset` creation use the worksheet provenance retained in the dataset and
preserve the same context.

Force a format or preserve source headers:

```rust,no_run
use electrodata_io::{ColumnNamePolicy, FormatId, ReadOptions, read_with_options};
let options = ReadOptions::new()
    .with_format(FormatId::RegularXy)
    .with_column_name_policy(ColumnNamePolicy::PreserveSource);
let dataset = read_with_options("table.csv", &options)?;
# Ok::<(), electrodata_io::Error>(())
```

Enable non-mutating scientific validation:

```rust,no_run
use electrodata_io::{ReadOptions, ValidationLevel, read_with_options};
let options = ReadOptions::new().with_validation(ValidationLevel::Scientific);
let dataset = read_with_options("EIS.csv", &options)?;
for diagnostic in dataset.diagnostics { println!("{}", diagnostic.message); }
# Ok::<(), electrodata_io::Error>(())
```

## Registering a format

Implement `FormatHandler` and register it. The handler detects a bounded
`Probe`, returns a `DetectedLayout`, and describes semantic columns; the shared
pipeline performs metadata retention, strict numeric loading, naming, and
validation.

```rust,no_run
use electrodata_io::{DataReader, Detection, FormatHandler, FormatId,
    Probe, ReadOptions, ColumnDescriptor};

struct MyCustomFormatHandler;

impl FormatHandler for MyCustomFormatHandler {
    fn format_id(&self) -> FormatId { FormatId::Custom("my-format".into()) }
    fn detect(&self, probe: &Probe, options: &ReadOptions) -> Option<Detection> {
        // Inspect only `probe.rows`; return a content-based detection.
        # let _ = (probe, options);
        None
    }
    fn columns(&self, probe: &Probe, detection: &Detection)
        -> electrodata_io::Result<Vec<ColumnDescriptor>> {
        # let _ = (probe, detection);
        # Ok(Vec::new())
    }
}

let reader = DataReader::builder()
    .with_builtin_formats()
    .register(MyCustomFormatHandler)
    .build();
```

No existing handler or central dispatch table changes are needed.

## Features

- `csv` (default): delimited UTF-8 input
- `excel` (default): `.xlsx` compatible time-series worksheets through Calamine
- `scientific-validation` (default): domain diagnostics
- `serde`: serialization for metadata, options, diagnostics, and schema types;
  `DataFrame` serialization is deliberately independent

## Dependency

From a local checkout:

```toml
[dependencies]
electrodata-io = { path = "../electrodata-io" }
```

For example (on Mac):

```toml
[dependencies]
electrodata-io = { path = "/Users/username/ProjectPath/electrodata-io" }
```

Or, once published:

```toml
[dependencies]
electrodata-io = "0.1"
```

The minimum supported Rust version is 1.88.

## Extended usage guide

### Requirements and dependency setup

The crate requires Rust 1.88 or newer. The default features provide CSV,
Excel, and scientific validation support.

For a published dependency:

```bash
cargo add electrodata-io@0.1
```

```toml
[dependencies]
electrodata-io = "0.1"
```

For a local checkout:

```bash
cargo add electrodata-io \
  --path /Users/xingyuwang/ProjectOngoing/electrodata-io
```

```toml
[dependencies]
electrodata-io = { path = "/Users/xingyuwang/ProjectOngoing/electrodata-io" }
```

For a Git dependency, pin a tag or commit for reproducibility:

```toml
[dependencies]
electrodata-io = {
    git = "https://github.com/xingyuwang/electrodata-io",
    tag = "v0.1.0"
}
```

The library is imported from the consuming application; it does not provide a
CLI or an application entry point:

```rust
use electrodata_io::{read, read_with_options, DataReader, Dataset, Error, Result};
```

### Reading and reusing a reader

Use `read` for a one-off load:

```rust,no_run
use electrodata_io::read;

fn load(path: &str) -> Result<(), electrodata_io::Error> {
    let dataset = read(path)?;
    println!("{}: {:?}", dataset.format, dataset.data.shape());
    Ok(())
}
```

Use a configured `DataReader` for repeated reads or custom handlers:

```rust,no_run
use electrodata_io::DataReader;

let reader = DataReader::builder().with_builtin_formats().build();
let first = reader.read("run-1.csv")?;
let second = reader.read("run-2.csv")?;
# let _ = (first, second);
# Ok::<(), electrodata_io::Error>(())
```

### Format-specific usage

#### Regular XY CSV

Both forms are supported:

```text
x,y
0.0,1.2
0.1,1.3
```

```text
0.0,1.2
0.1,1.3
```

Canonical columns are `x` and `y`; every numeric column is `Float64`:

```rust,no_run
use electrodata_io::read;

let dataset = read("regular_file.csv")?;
let x = dataset.data.column("x")?.f64()?;
let y = dataset.data.column("y")?.f64()?;
println!("first point = ({:?}, {:?})", x.get(0), y.get(0));
# Ok::<(), electrodata_io::Error>(())
```

#### CHI OCPT

CHI Open Circuit Potential is detected from the technique and header roles,
including `Time/sec` and `Time/s`. Canonical columns are `time_s` and
`potential_v`:

```rust,no_run
use electrodata_io::{read, ColumnRole};

let dataset = read("OCPT.csv")?;
let time = dataset.column_by_role(&ColumnRole::Time)?.f64()?;
let potential = dataset.column_by_role(&ColumnRole::Potential)?.f64()?;
println!("{} samples; first potential: {:?}", time.len(), potential.get(0));
# Ok::<(), electrodata_io::Error>(())
```

#### CHI multichannel OCPT

Numbered headers such as `E1/V`, `E5/V`, `CH1/V`, `CH5/V`, `CH1`, and `CH5`
are matched case-insensitively after trimming. Any positive channel number is
accepted, including noncontiguous channels:

```rust,no_run
use electrodata_io::{read, ColumnRole};

let dataset = read("Multichannel_OCPT.csv")?;
let time = dataset.column_by_role(&ColumnRole::Time)?;
let channel_1 = dataset.channel(1)?;
let channel_5 = dataset.channel(5)?;
println!("rows={}, ch1={}, ch5={}", time.len(), channel_1.len(), channel_5.len());
# Ok::<(), electrodata_io::Error>(())
```

Duplicate timestamps and source order are preserved. No sorting, deduplication,
aggregation, interpolation, or resampling occurs.

#### CHI EIS

The canonical EIS columns are `frequency_hz`, `z_real_ohm`, `z_imag_ohm`,
`z_magnitude_ohm`, and `phase_deg`:

```rust,no_run
use electrodata_io::{read, ColumnRole};

let dataset = read("EIS.csv")?;
let frequency = dataset.column_by_role(&ColumnRole::Frequency)?.f64()?;
let imaginary = dataset.column_by_role(&ColumnRole::ImpedanceImaginary)?.f64()?;
let phase = dataset.column_by_role(&ColumnRole::ImpedancePhase)?.f64()?;
println!("first frequency: {:?} Hz", frequency.get(0));
println!("first imaginary impedance: {:?} ohm", imaginary.get(0));
println!("first phase: {:?} degrees", phase.get(0));
# Ok::<(), electrodata_io::Error>(())
```

Descending frequencies remain descending. Imaginary impedance remains negative
when recorded as negative; plotting code may explicitly use `-z_imag_ohm`.

### Configuration patterns

Force a header policy, format, or source-preserving names:

```rust,no_run
use electrodata_io::{
    read_with_options, ColumnNamePolicy, FormatId, HeaderPolicy, ReadOptions,
};

let options = ReadOptions::new()
    .with_header_policy(HeaderPolicy::Absent)
    .with_format(FormatId::RegularXy)
    .with_column_name_policy(ColumnNamePolicy::PreserveSource);
let dataset = read_with_options("known-table.csv", &options)?;
# let _ = dataset;
# Ok::<(), electrodata_io::Error>(())
```

Canonical names are the default. `Dataset::columns` always retains the source
heading, canonical name, semantic role, and parsed unit mapping.

### Direct Polars and metadata access

`Dataset::data` is an ordinary Polars `DataFrame`:

```rust,no_run
use electrodata_io::read;

let dataset = read("experiment.csv")?;
println!("height={}, width={}", dataset.data.height(), dataset.data.width());
println!("schema={:?}", dataset.data.schema());
# Ok::<(), electrodata_io::Error>(())
```

Typed acquisition fields, parsed parameters, and every raw metadata row are
retained:

```rust,no_run
use electrodata_io::read;

let dataset = read("OCPT.csv")?;
println!("technique: {:?}", dataset.metadata.acquisition.technique);
for parameter in &dataset.metadata.parameters {
    println!("{} = {:?} (raw {:?})", parameter.name, parameter.value, parameter.raw_value);
}
for row in &dataset.metadata.raw_rows {
    println!("row {}: {}", row.row_number, row.reconstructed_text);
}
# Ok::<(), electrodata_io::Error>(())
```

CHI timestamps are `chrono::NaiveDateTime`; the reader does not invent a
timezone. Bare flags such as `Imp SF` are retained as parameters.

## Validation and diagnostics

Structural validation is enabled by default. It checks nonempty data, consistent
row widths, required roles, and numeric columns. Entirely-null required
coordinate/EIS columns are rejected, while entirely-null auxiliary columns may
remain. Scientific validation is opt-in:

```rust,no_run
use electrodata_io::{read_with_options, ReadOptions, ValidationLevel};

let options = ReadOptions::new().with_validation(ValidationLevel::Scientific);
let dataset = read_with_options("EIS.csv", &options)?;
for diagnostic in &dataset.diagnostics {
    println!("{:?}: {}", diagnostic.severity, diagnostic.message);
}
# Ok::<(), electrodata_io::Error>(())
```

Scientific checks include finite and generally nondecreasing OCPT time,
duplicate-time warnings, positive and monotonic EIS frequencies, and tolerance-
based EIS magnitude and phase comparisons. Validation never rewrites measured
values. Use `ValidationLevel::None` only when the application performs its own
checks.

## Excel worksheet selection

`.xlsx` is supported for compatible time-series worksheets. Legacy OLE `.xls`
is explicitly unsupported and should be converted to `.xlsx` or a delimited
text format. Automatic selection probes every nonempty worksheet: it selects
the worksheet only when exactly one is compatible, and otherwise returns a
structured worksheet ambiguity error. Detection confidence is retained as
diagnostic evidence but never breaks a worksheet-selection tie. Select a sheet
explicitly by name or zero-based index:

```rust,no_run
use electrodata_io::{read_with_options, ReadOptions, SheetSelector};

let by_name = ReadOptions::new().with_sheet(SheetSelector::Name("Experiment 2".into()));
let dataset = read_with_options("measurements.xlsx", &by_name)?;
println!("selected: {:?}", dataset.metadata.provenance.worksheet);

let by_index = ReadOptions::new().with_sheet(SheetSelector::Index(1));
let second = read_with_options("measurements.xlsx", &by_index)?;
# let _ = second;
# Ok::<(), electrodata_io::Error>(())
```

The selected worksheet is recorded in provenance.

## Feature selection

Available features are:

- `csv`: CSV, TSV, and delimited text input.
- `excel`: `.xlsx` compatible time-series worksheets through Calamine; legacy
  OLE `.xls` is explicitly unsupported.
- `scientific-validation`: OCPT and EIS domain diagnostics.
- `serde`: serialization for metadata, options, diagnostics, and schema types.

For a CSV-only application:

```toml
[dependencies]
electrodata-io = { version = "0.1", default-features = false, features = ["csv"] }
```

## Deploy or publish an application

This crate is a library dependency. Deployment normally means deploying the
consuming application. Pin dependency versions or Git revisions, commit
`Cargo.lock`, and build in release mode:

```bash
cargo update
cargo test --locked --all-features
cargo build --release --locked
```

Keep input files outside the binary and pass their paths to `read` or
`read_with_options` at runtime. A consuming application can use a multi-stage
container build:

```dockerfile
FROM rust:1.88-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /app/target/release/my-reader /usr/local/bin/my-reader
ENTRYPOINT ["/usr/local/bin/my-reader"]
```

Replace `my-reader` with the consuming binary name. Keep the `excel` feature
enabled when the deployed application reads workbooks.

## Publish this crate

Before publishing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps --all-features
cargo package --allow-dirty --list
cargo publish --dry-run
cargo publish
```

The manifest includes source, tests, documentation, changelog, and licenses
while excluding the large local fixture files under `data/`. Update the SemVer
version and `CHANGELOG.md` before each release.

## Error handling

Use `Result<Dataset, electrodata_io::Error>` at the application boundary:

```rust,no_run
use electrodata_io::{read, Error};

match read("input.csv") {
    Ok(dataset) => println!("loaded {} rows", dataset.data.height()),
    Err(Error::MalformedNumeric { row, column, value, .. }) => {
        eprintln!("invalid value {value:?} at row {row}, column {column}");
    }
    Err(error) => eprintln!("could not load input: {error}"),
}
```

### Structured EIS failures

Fatal EIS failures remain machine-readable through `read`, `read_with_options`,
and `Dataset::eis_view()`. Callers can match these public variants:

- `MissingRequiredRole` carries the path, `ChiEis` format candidate, absent
  role, all detected roles, and source-located `EisSchema` diagnostics.
- `EisSchemaConflict` carries an `EisSchemaError` with the path, format,
  one-based header row and column, original header, candidate roles, optional
  duplicate role, and rejection diagnostics.
- `UnknownUnit` and `IncompatibleUnit` carry the path, required role, one-based
  column, original source header, units, and expected target unit where
  applicable.
- `InvalidDatasetView` carries the path, requested view, and actual dataset
  kind. `MissingValue` carries the required role plus dataset/source row and
  source-column context.
- Fatal row decoding remains typed as variants such as `MalformedNumeric` and
  `RaggedRow`, including path, format, row, column, diagnostics, and an
  underlying source error when one exists.

Fatal errors return `Err(Error)` and prevent construction of an invalid EIS
dataset. Recoverable conditions return a `Dataset` and remain in
`Dataset::diagnostics` with a stable `DiagnosticCode`, location, severity, and
recovery action; diagnostics are never substitutes for fatal schema errors.
Structural EIS variants are terminal and normally have no `Error::source()`.
Backend and parsing variants preserve their lower-level error through
`std::error::Error::source()`.

There is no public `Error` enum layout change for this correction. The only
runtime compatibility change is for invalid files with an explicit EIS
technique and a two-column EIS header: they now return
`MissingRequiredRole` instead of a generic detection or column-signature
failure. Valid EIS and regular-table parsing are unchanged.

The library does not panic on malformed input. Strict UTF-8 decoding is the
default; explicitly enabling lossy decoding adds a decoding diagnostic.

## Development and verification

From a checkout, run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps --all-features
```

The integration tests cover supplied fixtures, generated regular and CHI
tables, numbered channels, duplicate timestamps, EIS sign/order preservation,
malformed and ragged rows, BOM and line endings, custom handlers, and Excel
worksheet selection.
