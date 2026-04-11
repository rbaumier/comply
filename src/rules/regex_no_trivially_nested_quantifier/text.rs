use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects trivially nested quantifiers that can be merged.
/// Example: `(?:a{2}){3}` can be `a{6}`, or `(?:a+)?` can be `a*`.
fn find_trivially_nested_quantifiers(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for `(?:` non-capturing group
        if bytes[i] == b'('
            && i + 2 < len
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b':'
        {
            let group_start = i;
            let content_start = i + 3;
            let mut depth = 1;
            let mut j = content_start;

            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            // j is now one past the closing paren
            let close = j - 1; // position of ')'
            if depth == 0 {
                let content = &line[content_start..close];
                // Check: single element with quantifier, e.g. `a+`, `a*`, `a?`, `a{2}`
                let has_inner_quantifier = is_single_quantified_element(content);
                // Check outer quantifier
                if has_inner_quantifier && close + 1 < len {
                    let next = bytes[close + 1];
                    if next == b'+' || next == b'*' || next == b'?' || next == b'{' {
                        hits.push(group_start);
                    }
                }
            }
        }
        i += 1;
    }
    hits
}

/// Returns true if the content is a single element followed by a quantifier.
/// E.g. `a+`, `a*`, `.?`, `a{2,3}`, `\d+`
fn is_single_quantified_element(content: &str) -> bool {
    let bytes = content.as_bytes();
    let clen = bytes.len();
    if clen < 2 {
        return false;
    }

    // Determine element length
    let elem_len;
    if bytes[0] == b'\\' {
        // Escaped char like `\d`, `\w`, `\s`
        elem_len = 2;
    } else if bytes[0] == b'[' {
        // Character class
        if let Some(close) = find_char_class_close(bytes, 0) {
            elem_len = close + 1;
        } else {
            return false;
        }
    } else if bytes[0] == b'.' || bytes[0].is_ascii_alphanumeric() {
        elem_len = 1;
    } else {
        return false;
    }

    if elem_len >= clen {
        return false;
    }

    // Rest must be a quantifier
    let rest = bytes[elem_len];
    match rest {
        b'+' | b'*' | b'?' => elem_len + 1 == clen || (elem_len + 2 == clen && bytes[elem_len + 1] == b'?'),
        b'{' => bytes[elem_len..].contains(&b'}'),
        _ => false,
    }
}

fn find_char_class_close(bytes: &[u8], start: usize) -> Option<usize> {
    let mut j = start + 1;
    if j < bytes.len() && bytes[j] == b'^' {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b']' {
        j += 1; // literal ] at start of class
    }
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_trivially_nested_quantifiers(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-trivially-nested-quantifier".into(),
                    message: "Trivially nested quantifiers can be merged into a single quantifier.".into(),
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
    fn flags_nested_plus_optional() {
        assert_eq!(run(r#"const re = /(?:a+)?/;"#).len(), 1);
    }

    #[test]
    fn allows_multi_element_group() {
        assert!(run(r#"const re = /(?:ab)+/;"#).is_empty());
    }

    #[test]
    fn flags_nested_star_plus() {
        assert_eq!(run(r#"const re = /(?:a*)+/;"#).len(), 1);
    }
}
