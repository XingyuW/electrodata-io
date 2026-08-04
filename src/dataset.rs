//! Unified read result.

use crate::{ColumnDescriptor, ColumnRole, Diagnostic, Error, FormatId, Metadata, Result};
use polars::prelude::{Column, DataFrame};

/// A scientific table and all information retained while reading it.
#[derive(Debug, Clone)]
pub struct Dataset {
    /// Numeric data, directly accessible to Polars callers.
    pub data: DataFrame,
    /// Raw and typed metadata.
    pub metadata: Metadata,
    /// Detected scientific format.
    pub format: FormatId,
    /// Source-to-canonical schema mappings.
    pub columns: Vec<ColumnDescriptor>,
    /// Non-fatal decoding and validation reports.
    pub diagnostics: Vec<Diagnostic>,
}

impl Dataset {
    /// Returns a DataFrame column with the requested semantic role.
    pub fn column_by_role(&self, role: &ColumnRole) -> Result<&Column> {
        let descriptor = self
            .columns
            .iter()
            .find(|item| &item.role == role)
            .ok_or_else(|| Error::ColumnRoleNotFound(role.clone()))?;
        let names = self.data.get_column_names();
        let displayed = names
            .iter()
            .find(|name| {
                name.as_str() == descriptor.canonical_name
                    || name.as_str() == descriptor.source_name.trim()
            })
            .ok_or_else(|| Error::ColumnRoleNotFound(role.clone()))?;
        Ok(self.data.column(displayed.as_str())?)
    }

    /// Returns a numbered potential channel.
    pub fn channel(&self, channel: u32) -> Result<&Column> {
        self.column_by_role(&ColumnRole::PotentialChannel(channel))
    }
}
