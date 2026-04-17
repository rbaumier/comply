//! Resolve a `(line, column)` pair to a byte `(offset, length)` pair suitable
//! for miette's labeled spans. Used as the fallback for diagnostics without a
//! pre-captured span — primarily delegated diagnostics (oxlint/clippy/knip/
//! madge) that only carry line/col from external JSON output.
//!
//! The returned length covers the *rest of the line* from the reported column,
//! which matches whole-line highlighting semantics.

/// Returns `Some((offset, length))`, or `None` if the line doesn't exist in
/// `source`. Handles LF and CRLF line endings. Columns are 1-based (as they
/// come out of diagnostics).
#[must_use]
pub fn resolve_line_span(source: &str, line: usize, column: usize) -> Option<(usize, usize)> {
    if line == 0 {
        return None;
    }
    let bytes = source.as_bytes();
    let mut offset = 0usize;
    let mut current_line = 1usize;
    while current_line < line {
        let nl = bytes[offset..].iter().position(|&b| b == b'\n')?;
        offset += nl + 1;
        current_line += 1;
    }
    // `offset` now points at the first byte of the target line.
    let line_end = bytes[offset..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| offset + p)
        .unwrap_or(bytes.len());
    // Strip trailing \r for CRLF line endings.
    let line_end = if line_end > offset && bytes[line_end - 1] == b'\r' {
        line_end - 1
    } else {
        line_end
    };
    let col_offset = column.saturating_sub(1);
    let start = (offset + col_offset).min(line_end);
    Some((start, line_end - start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_col_1() {
        assert_eq!(resolve_line_span("abc\ndef\n", 1, 1), Some((0, 3)));
    }

    #[test]
    fn second_line_col_2() {
        assert_eq!(resolve_line_span("abc\ndef\n", 2, 2), Some((5, 2)));
    }

    #[test]
    fn crlf_strips_carriage_return() {
        assert_eq!(resolve_line_span("abc\r\ndef\r\n", 1, 1), Some((0, 3)));
        assert_eq!(resolve_line_span("abc\r\ndef\r\n", 2, 1), Some((5, 3)));
    }

    #[test]
    fn line_out_of_range_returns_none() {
        assert!(resolve_line_span("one\ntwo\n", 99, 1).is_none());
    }

    #[test]
    fn column_past_end_clamps_to_line_end() {
        assert_eq!(resolve_line_span("short\n", 1, 100), Some((5, 0)));
    }

    #[test]
    fn line_zero_returns_none() {
        assert!(resolve_line_span("anything", 0, 1).is_none());
    }
}
