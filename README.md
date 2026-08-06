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

## Basic use

```rust,no_run
use electrodata_io::{ColumnRole, read};

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
let time = dataset.f64_values(&ColumnRole::Time)?;
let view = dataset.time_series_view()?;
println!("{} samples, {} measurements", time.len(), view.measurements.len());
println!("time in seconds: {:?}", view.time_seconds);
# Ok::<(), electrodata_io::Error>(())
```

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
- `excel` (default): XLSX, XLS, XLSB, and ODS through Calamine
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
row widths, required roles, numeric columns, and columns that are entirely
null. Scientific validation is opt-in:

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

Calamine reads XLSX, XLS, XLSB, and ODS workbooks. Automatic selection probes
nonempty eligible sheets and chooses the strongest format candidate. Select a
sheet explicitly by name or zero-based index:

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

Automatic selection returns an ambiguity error when multiple worksheets are
equally plausible. The selected worksheet is recorded in provenance.

## Feature selection

Available features are:

- `csv`: CSV, TSV, and delimited text input.
- `excel`: XLSX, XLS, XLSB, and ODS through Calamine.
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
    Err(Error::MalformedNumeric { row, column, value }) => {
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
