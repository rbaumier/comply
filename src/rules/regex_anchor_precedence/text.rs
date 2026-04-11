use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extracts the pattern from a regex literal `/pattern/flags`.
/// Returns (pattern, start_col) pairs found on the line.
fn extract_regex_patterns(line: &str) -> Vec<(String, usize)> {
    let mut patterns = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'/' && is_regex_start(line, i) {
            let start = i;
            i += 1;
            let pat_start = i;
            // Scan until closing `/`
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'/' {
                    let pattern = line[pat_start..i].to_string();
                    patterns.push((pattern, start));
                    i += 1;
                    // Skip flags
                    while i < len && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    break;
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    patterns
}

fn is_regex_start(line: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let before = line[..pos].trim_end();
    if before.is_empty() {
        return true;
    }
    let last = before.as_bytes()[before.len() - 1];
    matches!(last, b'=' | b'(' | b',' | b'|' | b'!' | b':' | b';' | b'{' | b'[' | b'&')
}

/// Checks if a regex pattern has an anchor precedence issue.
/// Flags `^X|Y` (caret only on first alternative) or `X|Y$` (dollar only on last).
fn has_anchor_precedence_issue(pattern: &str) -> bool {
    // Must have alternation at the top level (not inside a group)
    let top_level_pipe = find_top_level_pipes(pattern);
    if top_level_pipe.is_empty() {
        return false;
    }

    // Split by top-level pipes
    let mut alternatives = Vec::new();
    let mut prev = 0;
    for &pipe_pos in &top_level_pipe {
        alternatives.push(&pattern[prev..pipe_pos]);
        prev = pipe_pos + 1;
    }
    alternatives.push(&pattern[prev..]);

    if alternatives.len() < 2 {
        return false;
    }

    let first = alternatives[0];
    let last = alternatives[alternatives.len() - 1];

    // Check: `^X|Y` — first alternative starts with `^` but others don't
    if first.starts_with('^') {
        let others_have_caret = alternatives[1..].iter().all(|a| a.starts_with('^'));
        if !others_have_caret {
            return true;
        }
    }

    // Check: `X|Y$` — last alternative ends with `$` but others don't
    if last.ends_with('$') && !last.ends_with("\\$") {
        let others_have_dollar = alternatives[..alternatives.len() - 1]
            .iter()
            .all(|a| a.ends_with('$') && !a.ends_with("\\$"));
        if !others_have_dollar {
            return true;
        }
    }

    false
}

fn find_top_level_pipes(pattern: &str) -> Vec<usize> {
    let mut pipes = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut depth = 0;
    let mut bracket_depth = 0;
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'\\' => { i += 2; continue; }
            b'(' => depth += 1,
            b')' => { if depth > 0 { depth -= 1; } }
            b'[' => bracket_depth += 1,
            b']' => { if bracket_depth > 0 { bracket_depth -= 1; } }
            b'|' if depth == 0 && bracket_depth == 0 => pipes.push(i),
            _ => {}
        }
        i += 1;
    }
    pipes
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for (pattern, col) in extract_regex_patterns(line) {
                if has_anchor_precedence_issue(&pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "regex-anchor-precedence".into(),
                        message: "Anchor in alternation may not bind as expected \u{2014} use `/^(a|b)$/` instead of `/^a|b$/`.".into(),
                        severity: Severity::Warning,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_caret_only_on_first() {
        let diags = run(r#"const re = /^foo|bar/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_dollar_only_on_last() {
        let diags = run(r#"const re = /foo|bar$/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_anchored_group() {
        assert!(run(r#"const re = /^(foo|bar)$/;"#).is_empty());
    }

    #[test]
    fn allows_all_anchored() {
        assert!(run(r#"const re = /^foo$|^bar$/;"#).is_empty());
    }

    #[test]
    fn allows_no_alternation() {
        assert!(run(r#"const re = /^foo$/;"#).is_empty());
    }
}
