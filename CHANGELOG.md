# Changelog

All notable changes follow [Keep a Changelog](https://keepachangelog.com/) and
this project uses [Semantic Versioning](https://semver.org/).

## [Unreleased] - planned 0.2.0

This release is a semver-minor breaking release in the pre-1.0 series. The
canonical read contract requires source-incompatible public schema and error
growth that cannot be represented safely by aliases alone.

### Added

- Canonical `DatasetSchema`, `DatasetKind`, and null-aware physical `Column`
  abstractions with typed time-series and EIS views.
- General N-column time-series ingestion, semantic roles/units, headerless
  compatibility inference, and source-row identity.
- Explicit invalid-coordinate, invalid-cell, ragged-row, and coordinate-order
  policies with strict defaults and a compatibility profile.
- Source-located diagnostics with stable codes and recovery actions.
- Role-based three-, four-, five-, and reordered-column EIS detection, including
  lower-confidence header-only detection.
- Content/magic-aware container dispatch, `.dat` text support, and explicit
  unsupported-binary errors.

### Compatibility

- The existing Polars `Dataset::data`, schema-descriptor `Dataset::columns`,
  direct built-in readers, legacy role variants, and `FormatHandler` contract
  remain available. Direct legacy readers use the compatibility profile.
- `ReadOptions` gained explicit recovery policies and is now non-exhaustive;
  downstream struct literals must migrate to `ReadOptions::new()` or
  `ReadOptions::compatibility()` plus builder methods.
- `Error` variants now retain path, format, source error, diagnostics, and
  source-cell context. Downstream destructuring must use accessor methods such
  as `context()`, `path()`, `row()`, and `column()` and include a wildcard when
  matching the non-exhaustive enum.
- `MissingRequiredRole` now exposes all detected roles. `UnknownUnit` and
  `IncompatibleUnit` retain the original source header, and `eis_view()` uses
  the additive `InvalidDatasetView` error to retain the dataset path and
  requested view when called for a non-EIS dataset.
- Canonical `ColumnRole` variants were added. Exhaustive downstream matches must
  include a wildcard. Legacy `Phase` and `PotentialChannel(n)` values remain
  equality-compatible with `ImpedancePhase` and `MeasurementChannel(n)`, but
  their serde spellings and Rust patterns remain distinct during migration.
- Legacy binary `.xls` is now an explicit path-bearing unsupported-container
  error. `.xlsx` remains supported through the canonical read pipeline.

## [0.1.0] - 2026-08-03

### Added

- Registry-based CSV and Excel scientific-data reader.
- Regular XY, CHI OCPT, CHI multichannel OCPT, and CHI EIS handlers.
- Lossless raw and typed metadata, provenance, semantic columns, diagnostics,
  structural validation, and optional scientific validation.
