use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Scan for `.replace(/.../<flags-with-g>,` patterns — a `.replace()` call whose
/// first argument is a regex literal with the global flag.
fn has_replace_with_global_regex(line: &str) -> bool {
    let bytes = line.as_bytes();

    // Look for `.replace(` occurrences
    let pattern = ".replace(";
    let mut start = 0;
    while start + pattern.len() <= bytes.len() {
        let Some(rel) = line[start..].find(pattern) else {
            break;
        };
        let abs = start + rel;
        let after_paren = abs + pattern.len();

        // Skip whitespace after `(`
        let mut i = after_paren;
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }

        // Must start with `/` (regex literal)
        if i >= bytes.len() || bytes[i] != b'/' {
            start = after_paren;
            continue;
        }
        i += 1;

        // Find closing `/`, respecting escapes
        let mut found_close = false;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if bytes[i] == b'/' {
                found_close = true;
                i += 1;
                break;
            }
            i += 1;
        }

        if !found_close {
            start = after_paren;
            continue;
        }

        // Collect flags
        let mut flags = String::new();
        while i < bytes.len() && bytes[i].is_ascii_lowercase() {
            flags.push(bytes[i] as char);
            i += 1;
        }

        // Must have `g` flag
        if !flags.contains('g') {
            start = after_paren;
            continue;
        }

        // Skip whitespace, then expect `,`
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b',' {
            return true;
        }

        start = after_paren;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_replace_with_global_regex(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-string-replace-all".into(),
                    message:
                        "Prefer `String#replaceAll()` over `String#replace()` with a global regex."
                            .into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }

    #[test]
    fn flags_replace_with_global_regex() {
        let d = run(r#"str.replace(/foo/g, 'bar')"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_replace_with_gu_flags() {
        let d = run(r#"str.replace(/foo/gu, 'bar')"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_replace_without_global() {
        assert!(run(r#"str.replace(/foo/, 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_with_string_arg() {
        assert!(run(r#"str.replace('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_all_already() {
        assert!(run(r#"str.replaceAll('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn flags_replace_with_case_insensitive_global() {
        let d = run(r#"str.replace(/foo/gi, 'bar')"#);
        assert_eq!(d.len(), 1);
    }
}
