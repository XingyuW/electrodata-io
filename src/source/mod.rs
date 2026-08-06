//! Storage containers and bounded probes.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Physical storage format, separate from scientific semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ContainerFormat {
    /// Delimited text.
    Csv,
    /// Office Open XML workbook.
    Xlsx,
    /// Legacy binary Excel workbook.
    Xls,
    /// Binary Excel workbook.
    Xlsb,
    /// OpenDocument spreadsheet.
    Ods,
}

/// A bounded, storage-neutral view used for format detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// Initial parsed rows.
    pub rows: Vec<Vec<String>>,
    /// Initial original rows without line endings.
    pub raw_rows: Vec<String>,
    /// Basename or logical source name.
    pub source_name: String,
    /// Delimiter for text containers.
    pub delimiter: Option<u8>,
    /// Physical container.
    pub container: ContainerFormat,
    /// Workbook member name.
    pub worksheet_name: Option<String>,
}

impl Probe {
    /// Returns a row by zero-based index.
    pub fn row(&self, index: usize) -> Option<&[String]> {
        self.rows.get(index).map(Vec::as_slice)
    }
}

#[cfg(feature = "csv")]
pub(crate) mod csv_source;
#[cfg(feature = "excel")]
pub(crate) mod excel;

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
