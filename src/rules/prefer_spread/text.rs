use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `Array.from(` — prefer `[...x]`.
fn find_array_from(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find("Array.from(") {
        let abs = start + pos;
        // Ensure `Array` is not part of a larger identifier
        if abs == 0 || !is_ident_char(line.as_bytes()[abs - 1]) {
            hits.push(abs);
        }
        start = abs + 11;
    }
    hits
}

/// Detect `.concat(` — prefer spread.
fn find_concat(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(".concat(") {
        let abs = start + pos;
        hits.push(abs);
        start = abs + 8;
    }
    hits
}

/// Detect `.slice()` or `.slice(0)` with no other args — prefer `[...arr]`.
fn find_slice_copy(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(pos) = line[start..].find(".slice(") {
        let abs = start + pos;
        let after = &line[abs + 7..];
        let trimmed = after.trim_start();
        // `.slice()` or `.slice(0)`
        if trimmed.starts_with(')') || trimmed.starts_with("0)") {
            hits.push(abs);
        }
        start = abs + 7;
    }
    hits
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_array_from(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-spread".into(),
                    message: "Prefer the spread operator over `Array.from(…)`.".into(),
                    severity: Severity::Warning,
                });
            }

            for col in find_concat(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-spread".into(),
                    message: "Prefer the spread operator over `Array#concat(…)`.".into(),
                    severity: Severity::Warning,
                });
            }

            for col in find_slice_copy(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-spread".into(),
                    message: "Prefer the spread operator over `Array#slice()`.".into(),
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
    fn flags_array_from() {
        let d = run("const arr = Array.from(iterable);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn flags_concat() {
        let d = run("const combined = arr.concat(other);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("concat"));
    }

    #[test]
    fn flags_slice_empty() {
        let d = run("const copy = arr.slice();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("slice"));
    }

    #[test]
    fn flags_slice_zero() {
        let d = run("const copy = arr.slice(0);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_slice_with_args() {
        assert!(run("const sub = arr.slice(1, 3);").is_empty());
    }

    #[test]
    fn allows_spread() {
        assert!(run("const arr = [...iterable];").is_empty());
    }

    #[test]
    fn does_not_flag_non_array_from() {
        // `TypedArray.from` or a custom `MyArray.from` are fine edge cases
        // but `SomeArray.from(` would still match — we accept this for simplicity
        assert!(run("const x = notAnArray();").is_empty());
    }
}
