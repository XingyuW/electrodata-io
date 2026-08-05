//! Text normalization used only for content matching.

/// Normalizes spacing, case, and common punctuation variations without
/// discarding scientific punctuation such as `/`, `'`, `"`, `+`, or `-`.
pub(crate) fn normalize_for_match(value: &str) -> String {
    let normalized = value
        .trim()
        .trim_start_matches('\u{feff}')
        .replace(['–', '—'], "-")
        .replace('’', "'")
        .replace(['“', '”'], "\"")
        .to_ascii_lowercase();
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_for_match;

    #[test]
    fn preserves_scientific_header_punctuation() {
        assert_eq!(normalize_for_match("  Z’/ohm  "), "z'/ohm");
        assert_eq!(normalize_for_match("Z\"/ohm"), "z\"/ohm");
        assert_eq!(normalize_for_match("A.C.   Impedance"), "a.c. impedance");
    }
}
