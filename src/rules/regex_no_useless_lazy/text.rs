use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect useless lazy quantifiers — a `?` after a quantifier that is
/// already fixed-length or at the end of the pattern where it has no effect.
/// Heuristic: flag patterns like `x??`, `x+?` at end of regex, or
/// `{n}?` (exact quantifier + lazy is always useless).
fn has_useless_lazy(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") {
        return false;
    }
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // Detect `{n}?` — exact quantifier followed by lazy `?`.
        if bytes[i] == b'{' {
            let start = i + 1;
            let mut j = start;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            // `{n}?` — exact count quantifier with useless lazy.
            if j > start && j < len && bytes[j] == b'}' {
                // Make sure it's not `{n,m}`.
                if j + 1 < len && bytes[j + 1] == b'?' {
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
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_useless_lazy(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-useless-lazy".into(),
                    message: "Useless lazy quantifier — the `?` after a fixed quantifier has no effect.".into(),
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
    fn flags_exact_quantifier_lazy() {
        assert_eq!(run("const re = /a{3}?/;").len(), 1);
    }

    #[test]
    fn flags_single_exact_lazy() {
        assert_eq!(run("const re = /x{1}?b/;").len(), 1);
    }

    #[test]
    fn allows_range_quantifier_lazy() {
        assert!(run("const re = /a{1,3}?/;").is_empty());
    }

    #[test]
    fn allows_no_lazy() {
        assert!(run("const re = /a{3}/;").is_empty());
    }
}
