use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line contains `.split(/` or `.replace(/` with a pattern that can
/// match the empty string (contains `*`, `?`, or `{0,`).
fn has_empty_match_in_split_replace(line: &str) -> bool {
    let mut start = 0;
    while start < line.len() {
        // Find .split(/ or .replace(/
        let split_pos = line[start..].find(".split(/");
        let replace_pos = line[start..].find(".replace(/");

        let (method_end, found_start) = match (split_pos, replace_pos) {
            (Some(s), Some(r)) => {
                if s < r {
                    (start + s + 8, start + s) // .split(/ is 8 chars
                } else {
                    (start + r + 10, start + r) // .replace(/ is 10 chars
                }
            }
            (Some(s), None) => (start + s + 8, start + s),
            (None, Some(r)) => (start + r + 10, start + r),
            (None, None) => return false,
        };
        let _ = found_start;

        // Extract regex pattern until closing /
        let rest = &line[method_end..];
        let mut i = 0;
        let bytes = rest.as_bytes();
        let mut pattern_end = None;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'/' {
                pattern_end = Some(i);
                break;
            }
            i += 1;
        }

        if let Some(end) = pattern_end {
            let pattern = &rest[..end];
            // Check for zero-length quantifiers (not preceded by backslash in a simple way)
            if pattern.contains('*') || pattern.contains('{') && pattern.contains("{0,") {
                // Make sure pattern doesn't have anchors that prevent empty match
                if !is_fully_anchored(pattern) {
                    return true;
                }
            }
            // Check for standalone `?` (not `\?`, not `??`, not `+?`, not `*?`)
            let pbytes = pattern.as_bytes();
            for j in 0..pbytes.len() {
                if pbytes[j] == b'?' {
                    // Skip if escaped
                    if j > 0 && pbytes[j - 1] == b'\\' {
                        continue;
                    }
                    // Skip if part of a reluctant quantifier (*?, +?, ??)
                    if j > 0
                        && (pbytes[j - 1] == b'*' || pbytes[j - 1] == b'+' || pbytes[j - 1] == b'?')
                    {
                        continue;
                    }
                    // Skip if part of non-capturing group (?:
                    if j + 1 < pbytes.len() && pbytes[j + 1] == b':' {
                        continue;
                    }
                    if !is_fully_anchored(pattern) {
                        return true;
                    }
                }
            }
        }

        start = method_end;
    }
    false
}

/// Simple check: pattern is fully anchored if it starts with `^` and ends with `$`.
fn is_fully_anchored(pattern: &str) -> bool {
    pattern.starts_with('^') && pattern.ends_with('$')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_match_in_split_replace(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-empty-string-match".into(),
                    message: "Regex can match the empty string in `.split()` or `.replace()` — this may cause unexpected results.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_split_with_star() {
        assert_eq!(run(r#""abc".split(/a*/);"#).len(), 1);
    }

    #[test]
    fn flags_replace_with_optional() {
        assert_eq!(run(r#"str.replace(/x?/g, '-');"#).len(), 1);
    }

    #[test]
    fn flags_replace_with_star() {
        assert_eq!(run(r#"s.replace(/\s*/g, '');"#).len(), 1);
    }

    #[test]
    fn allows_split_with_plus() {
        assert!(run(r#""abc".split(/a+/);"#).is_empty());
    }

    #[test]
    fn allows_replace_with_anchored() {
        assert!(run(r#"s.replace(/^x*$/, '-');"#).is_empty());
    }
}
