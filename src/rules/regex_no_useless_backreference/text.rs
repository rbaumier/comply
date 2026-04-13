use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects backreferences that always resolve to the empty string because
/// they reference themselves (nested) or a forward group.
/// Example: `(\1)` or `\1(a)` (forward backreference).
fn find_useless_backrefs(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
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
                    if has_useless_backref(pattern) {
                        hits.push(i);
                    }
                    i = j;
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

fn has_useless_backref(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut group_count = 0;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if bytes[i + 1].is_ascii_digit() && bytes[i + 1] != b'0' {
                let ref_num = (bytes[i + 1] - b'0') as usize;
                // Forward reference: group hasn't been opened yet
                if ref_num > group_count {
                    return true;
                }
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'(' && (i + 1 >= bytes.len() || bytes[i + 1] != b'?') {
            group_count += 1;
            // Check for self-reference: `(\N)` where N == group_count
            let inner_start = i + 1;
            if inner_start + 1 < bytes.len()
                && bytes[inner_start] == b'\\'
                && bytes[inner_start + 1].is_ascii_digit()
            {
                let ref_num = (bytes[inner_start + 1] - b'0') as usize;
                if ref_num == group_count {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_useless_backrefs(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-backreference".into(),
                    message: "Backreference always resolves to the empty string \u{2014} it references itself or a forward group.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_forward_backreference() {
        assert_eq!(run(r#"const re = /\1(a)/;"#).len(), 1);
    }

    #[test]
    fn flags_self_reference() {
        assert_eq!(run(r#"const re = /(\1)/;"#).len(), 1);
    }

    #[test]
    fn allows_valid_backreference() {
        assert!(run(r#"const re = /(a)\1/;"#).is_empty());
    }
}
