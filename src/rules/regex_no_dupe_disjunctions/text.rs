use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects duplicate alternatives in a regex disjunction, e.g. `/a|b|a/`.
fn find_dupe_disjunctions(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for regex literal start
        if bytes[i] == b'/' {
            if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b')') {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    let pattern = &line[start..j];
                    if has_dupe_alternatives(pattern) {
                        hits.push(i);
                    }
                    i = j;
                    break;
                }
                if bytes[j] == b'\n' {
                    break;
                }
                j += 1;
            }
        }
        // Check RegExp constructor
        if i + 7 < len && &line[i..i + 7] == "RegExp("
            && let Some(pattern) = extract_string_arg(&line[i + 7..])
                && has_dupe_alternatives(pattern) {
                    hits.push(i);
                }
        i += 1;
    }
    hits
}

fn extract_string_arg(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let q = match bytes[0] {
        b'"' | b'\'' => bytes[0],
        _ => return None,
    };
    let inner = &s[1..];
    inner.find(q as char).map(|end| &inner[..end])
}

fn has_dupe_alternatives(pattern: &str) -> bool {
    // Split top-level alternatives (not inside groups)
    let alts = split_top_level_alternatives(pattern);
    if alts.len() < 2 {
        return false;
    }
    for i in 0..alts.len() {
        for j in (i + 1)..alts.len() {
            if alts[i] == alts[j] && !alts[i].is_empty() {
                return true;
            }
        }
    }
    false
}

fn split_top_level_alternatives(pattern: &str) -> Vec<&str> {
    let mut alts = Vec::new();
    let bytes = pattern.as_bytes();
    let mut depth = 0;
    let mut start = 0;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\\' => {} // next char is escaped, but we just skip
            b'(' => depth += 1,
            b')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b'[' => depth += 1,
            b']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b'|' if depth == 0 => {
                alts.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alts.push(&pattern[start..]);
    alts
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_dupe_disjunctions(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-dupe-disjunctions".into(),
                    message: "Duplicate alternative in regex disjunction \u{2014} remove the redundant branch.".into(),
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
    fn flags_duplicate_alternative() {
        assert_eq!(run(r#"const re = /foo|bar|foo/;"#).len(), 1);
    }

    #[test]
    fn allows_unique_alternatives() {
        assert!(run(r#"const re = /foo|bar|baz/;"#).is_empty());
    }

    #[test]
    fn flags_regexp_constructor_dupes() {
        assert_eq!(run(r#"const re = new RegExp("a|b|a");"#).len(), 1);
    }
}
