//! Per-line parsing for the comply-ignore mechanism.
//!
//! Splits a single source line into either:
//! - an above-line marker (suppress next line),
//! - a trailing marker (suppress current line),
//! - or nothing (no marker, marker inside a string literal, missing rule id).
//!
//! The string-literal heuristic counts unescaped `"` / `'` / `` ` `` quotes
//! in the prefix before the marker — odd count means we're inside an open
//! string and the marker should not be honored. Without this, code like
//! `const s = "// comply-ignore: ...";` would register a phantom suppression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::ignore_comments::payload;
use std::path::Path;

const MARKER: &str = "// comply-ignore:";

/// Outcome of parsing a single source line.
#[derive(Debug)]
pub struct LineParse {
    pub rule_id: String,
    /// Line number to insert into the suppressions map.
    pub target_line: usize,
    /// Diagnostic to emit if the comment was missing its justification.
    pub bad_ignore: Option<Diagnostic>,
}

/// Parse one source line. Returns None if no honored marker is present.
pub fn parse(path: &Path, line: &str, line_num: usize) -> Option<LineParse> {
    let marker_byte = line.find(MARKER)?;
    let prefix = &line[..marker_byte];

    if is_inside_string_literal(prefix) {
        return None;
    }

    let parsed = payload::parse(&line[marker_byte + MARKER.len()..]);
    if parsed.rule_id.is_empty() {
        return None;
    }

    // Trailing marker (code before it on the same line) suppresses THIS line;
    // above-line marker (only whitespace before it) suppresses the NEXT line.
    let is_trailing = !prefix.trim_start().is_empty();
    let target_line = if is_trailing { line_num } else { line_num + 1 };

    let bad_ignore = if parsed.justification.is_empty() {
        let col = prefix.chars().count();
        Some(make_bad_ignore_diagnostic(path, line_num, col, &parsed.rule_id))
    } else {
        None
    };
    Some(LineParse {
        rule_id: parsed.rule_id,
        target_line,
        bad_ignore,
    })
}

/// True if the prefix has an unmatched opening quote, suggesting the marker
/// is inside a string literal. Conservative heuristic — catches common cases
/// without parsing JS expression syntax.
fn is_inside_string_literal(prefix: &str) -> bool {
    let mut in_double = false;
    let mut in_single = false;
    let mut in_backtick = false;
    let mut chars = prefix.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next(); // Skip escaped character.
            }
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '`' if !in_double && !in_single => in_backtick = !in_backtick,
            _ => {}
        }
    }
    in_double || in_single || in_backtick
}

/// Construct a diagnostic for a comply-ignore comment missing its justification.
fn make_bad_ignore_diagnostic(
    path: &Path,
    line: usize,
    char_column: usize,
    rule_id: &str,
) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        line,
        column: char_column + 1,
        rule_id: "comply-ignore-missing-justification".into(),
        message: format!(
            "comply-ignore without justification — explain why this exception \
             is needed: `// comply-ignore: {rule_id} — <reason>`"
        ),
        severity: Severity::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn above_line_marker_targets_next_line() {
        let lp = parse(Path::new("t.ts"), "  // comply-ignore: no-throw — ok", 5).unwrap();
        assert_eq!(lp.target_line, 6);
    }

    #[test]
    fn trailing_marker_targets_current_line() {
        let lp =
            parse(Path::new("t.ts"), "throw err; // comply-ignore: no-throw — legacy", 5).unwrap();
        assert_eq!(lp.target_line, 5);
    }

    #[test]
    fn marker_inside_double_quoted_string_is_ignored() {
        assert!(parse(Path::new("t.ts"), "let s = \"// comply-ignore: no-throw — x\";", 1).is_none());
    }

    #[test]
    fn marker_inside_single_quoted_string_is_ignored() {
        assert!(parse(Path::new("t.ts"), "let s = '// comply-ignore: no-throw — x';", 1).is_none());
    }

    #[test]
    fn marker_inside_backtick_template_is_ignored() {
        assert!(parse(Path::new("t.ts"), "let s = `// comply-ignore: no-throw — x`;", 1).is_none());
    }
}
