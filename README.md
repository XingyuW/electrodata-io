# electrodata-io

`electrodata-io` is a reusable Rust library for reading electrochemical and
scientific tables into Polars. Storage detection (CSV or Excel) is independent
from scientific-format detection, and new scientific formats are registered as
object-safe handlers instead of being added to a central dispatch `match`.

The crate preserves source order, duplicate samples, numeric signs, raw
metadata, and original headers. It never sorts, deduplicates, resamples,
interpolates, corrects units, or replaces measured values with derived values.

## Basic use

```rust,no_run
use electrodata_io::read;

let dataset = read("experiment.csv")?;
println!("{} rows", dataset.data.height());
# Ok::<(), electrodata_io::Error>(())
```

`Dataset::data` is an ordinary Polars `DataFrame`:

```rust,no_run
let dataset = electrodata_io::read("experiment.csv")?;
let frame = &dataset.data;
println!("{:?}", frame.schema());
# Ok::<(), electrodata_io::Error>(())
```

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
let time = dataset.column_by_role(&ColumnRole::Time)?;
let channel_five = dataset.channel(5)?;
# Ok::<(), electrodata_io::Error>(())
```

EIS roles preserve the recorded imaginary sign and measured values:

```rust,no_run
use electrodata_io::ColumnRole;
let dataset = electrodata_io::read("EIS.csv")?;
let frequency = dataset.column_by_role(&ColumnRole::Frequency)?;
let z_imaginary = dataset.column_by_role(&ColumnRole::ImpedanceImaginary)?;
# Ok::<(), electrodata_io::Error>(())
```

## Options

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

Or, once published:

```toml
[dependencies]
electrodata-io = "0.1"
```

The minimum supported Rust version is 1.88.
