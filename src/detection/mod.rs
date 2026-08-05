//! Public automatic scientific-format detection.

mod normalize;

use crate::{Detection, Error, FormatId, ReadOptions, Result};
use std::path::Path;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub(crate) use normalize::normalize_for_match;

/// Built-in scientific formats that can be selected explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ElectroDataFormat {
    /// A rectangular two-column numeric table.
    Regular,
    /// CH Instruments open-circuit potential versus time.
    ChiOcpt,
    /// CH Instruments numbered-channel open-circuit potential versus time.
    ChiMultichannelOcpt,
    /// CH Instruments A.C. impedance data.
    ChiEis,
}

impl From<ElectroDataFormat> for FormatId {
    fn from(value: ElectroDataFormat) -> Self {
        match value {
            ElectroDataFormat::Regular => Self::RegularXy,
            ElectroDataFormat::ChiOcpt => Self::ChiOcpt,
            ElectroDataFormat::ChiMultichannelOcpt => Self::ChiMultichannelOcpt,
            ElectroDataFormat::ChiEis => Self::ChiEis,
        }
    }
}

impl TryFrom<&FormatId> for ElectroDataFormat {
    type Error = ();

    fn try_from(value: &FormatId) -> std::result::Result<Self, Self::Error> {
        match value {
            FormatId::RegularXy => Ok(Self::Regular),
            FormatId::ChiOcpt => Ok(Self::ChiOcpt),
            FormatId::ChiMultichannelOcpt => Ok(Self::ChiMultichannelOcpt),
            FormatId::ChiEis => Ok(Self::ChiEis),
            FormatId::Custom(_) => Err(()),
        }
    }
}

/// Content-based scientific-format detection details.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DetectionResult {
    /// Selected built-in scientific format.
    pub format: ElectroDataFormat,
    /// Confidence in the selected format, in the range `[0, 1]`.
    pub confidence: f32,
    /// Zero-based source header row, when one exists.
    pub header_row: Option<usize>,
    /// Zero-based first data row.
    pub data_start_row: usize,
    /// Number of data columns.
    pub column_count: usize,
    /// Original-text-derived evidence used for selection.
    pub matched_patterns: Vec<String>,
}

impl DetectionResult {
    pub(crate) fn from_detection(detection: &Detection) -> Option<Self> {
        Some(Self {
            format: ElectroDataFormat::try_from(&detection.format).ok()?,
            confidence: detection.confidence,
            header_row: detection.layout.header_row,
            data_start_row: detection.layout.data_start_row,
            column_count: detection.layout.expected_column_count,
            matched_patterns: detection.evidence.clone(),
        })
    }
}

pub(crate) fn select(
    path: &Path,
    mut candidates: Vec<Detection>,
    options: &ReadOptions,
) -> Result<DetectionResult> {
    candidates.retain(|candidate| ElectroDataFormat::try_from(&candidate.format).is_ok());
    candidates.sort_by(|left, right| right.confidence.total_cmp(&left.confidence));
    let best = candidates
        .first()
        .ok_or_else(|| Error::UnknownFormatDetailed {
            path: path.to_path_buf(),
            evidence: vec!["no registered built-in handler matched the bounded preview".into()],
        })?;
    if best.confidence < options.detection_threshold {
        return Err(Error::UnknownFormatDetailed {
            path: path.to_path_buf(),
            evidence: best.evidence.clone(),
        });
    }
    if candidates
        .get(1)
        .is_some_and(|second| best.confidence - second.confidence < options.detection_margin)
    {
        return Err(Error::AmbiguousDetection {
            candidates: candidates
                .iter()
                .take(3)
                .filter_map(DetectionResult::from_detection)
                .collect(),
        });
    }
    DetectionResult::from_detection(best).ok_or_else(|| Error::UnknownFormatDetailed {
        path: path.to_path_buf(),
        evidence: vec!["the only matching handler was not a built-in scientific format".into()],
    })
}

#[cfg(test)]
mod tests {
    use super::{ElectroDataFormat, select};
    use crate::{DetectedLayout, Detection, Error, FormatId, ReadOptions};
    use std::path::Path;

    fn candidate(format: FormatId, confidence: f32) -> Detection {
        Detection {
            format,
            confidence,
            evidence: vec!["test evidence".into()],
            layout: DetectedLayout {
                header_row: Some(1),
                data_start_row: 2,
                metadata_range: Some(0..1),
                expected_column_count: 2,
                has_header: true,
            },
        }
    }

    #[test]
    fn selects_the_highest_confidence_candidate() {
        let result = select(
            Path::new("sample.csv"),
            vec![
                candidate(FormatId::RegularXy, 0.70),
                candidate(FormatId::ChiOcpt, 0.95),
            ],
            &ReadOptions::default(),
        )
        .unwrap();
        assert_eq!(result.format, ElectroDataFormat::ChiOcpt);
        assert_eq!(result.column_count, 2);
    }

    #[test]
    fn reports_ambiguous_candidates_inside_the_configured_margin() {
        let error = select(
            Path::new("sample.csv"),
            vec![
                candidate(FormatId::RegularXy, 0.80),
                candidate(FormatId::ChiOcpt, 0.75),
            ],
            &ReadOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error, Error::AmbiguousDetection { .. }));
    }

    #[test]
    fn reports_unknown_when_no_candidate_exceeds_the_threshold() {
        let error = select(
            Path::new("sample.csv"),
            vec![candidate(FormatId::RegularXy, 0.50)],
            &ReadOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error, Error::UnknownFormatDetailed { .. }));
    }
}
