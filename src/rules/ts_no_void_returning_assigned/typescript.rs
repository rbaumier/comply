//! Flag `const x = console.log(...)` / `const x = arr.forEach(...)` and a
//! short list of other void-typical methods. Detection is pattern-based.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

/// Suffixes of the `<callee>(` form we treat as void-returning.
const VOID_PATTERNS: &[&str] = &[
    "console.log(",
    "console.error(",
    "console.warn(",
    "console.info(",
    "console.debug(",
    "console.table(",
    ".forEach(",
];

const ASSIGN_KEYWORDS: &[&str] = &["const ", "let ", "var "];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        'lines: for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            // Must start with const/let/var.
            let kw = ASSIGN_KEYWORDS.iter().find(|k| trimmed.starts_with(*k));
            let Some(kw) = kw else { continue };
            let leading = line.len() - trimmed.len();
            let after_kw = &trimmed[kw.len()..];
            // Find an `=` on this line.
            let Some(eq_rel) = after_kw.find('=') else { continue };
            let rhs = &after_kw[eq_rel + 1..];
            for pat in VOID_PATTERNS {
                if rhs.contains(pat) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: leading + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Storing the return of `{}` is always `undefined` — the call returns void.",
                            pat.trim_end_matches('(')
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    continue 'lines;
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
    fn flags_const_console_log() {
        let src = "const x = console.log('hi');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_const_foreach() {
        let src = "const r = arr.forEach(x => x + 1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_const_map() {
        let src = "const r = arr.map(x => x + 1);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_console_log() {
        let src = "console.log('hi');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_let_console_error() {
        let src = "let y = console.error('boom');";
        assert_eq!(run(src).len(), 1);
    }
}
