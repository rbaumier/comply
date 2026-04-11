use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `arr[arr.length - N]` pattern — prefer `.at(-N)`.
fn find_bracket_length_minus(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(bracket_pos) = line[start..].find('[') {
        let abs = start + bracket_pos;
        // Extract the receiver before `[`
        if let Some(receiver) = extract_ident_before(&line[..abs]) {
            let inner = &line[abs + 1..];
            let expected = format!("{}.length - ", receiver);
            let trimmed_inner = inner.trim_start();
            if trimmed_inner.starts_with(&expected) {
                // Verify there's a closing `]` after
                if inner.contains(']') {
                    hits.push(abs);
                }
            }
        }
        start = abs + 1;
    }
    hits
}

/// Detect `.charAt(` calls — prefer `.at()`.
fn find_char_at(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(".charAt(") {
        let abs = start + pos;
        hits.push(abs);
        start = abs + 8;
    }
    hits
}

/// Extract an identifier/path immediately before a position.
fn extract_ident_before(before: &str) -> Option<&str> {
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.len();
    let mut i = end;
    for ch in trimmed.chars().rev() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.' {
            i -= ch.len_utf8();
        } else {
            break;
        }
    }
    let ident = &trimmed[i..end];
    if ident.is_empty() || ident.starts_with('.') || ident.ends_with('.') {
        return None;
    }
    Some(ident)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Flag `arr[arr.length - N]`
            for col in find_bracket_length_minus(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-at".into(),
                    message: "Prefer `.at(…)` over `[….length - index]`.".into(),
                    severity: Severity::Warning,
                });
            }

            // Flag `.charAt(`
            for col in find_char_at(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-at".into(),
                    message: "Prefer `String#at(…)` over `String#charAt(…)`.".into(),
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
    fn flags_length_minus_bracket_access() {
        let d = run("const last = arr[arr.length - 1];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(".at("));
    }

    #[test]
    fn flags_char_at() {
        let d = run("const c = str.charAt(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("at("));
    }

    #[test]
    fn allows_at() {
        assert!(run("const last = arr.at(-1);").is_empty());
    }

    #[test]
    fn allows_normal_bracket_access() {
        assert!(run("const first = arr[0];").is_empty());
    }

    #[test]
    fn flags_nested_receiver() {
        let d = run("const x = foo.bar[foo.bar.length - 2];");
        assert_eq!(d.len(), 1);
    }
}
