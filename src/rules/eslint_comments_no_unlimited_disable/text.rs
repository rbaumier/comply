//! eslint-comments-no-unlimited-disable text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MARKERS: &[&str] = &[
    "eslint-disable",
    "eslint-disable-next-line",
    "eslint-disable-line",
];

impl TextCheck for Check {
    // Every marker is an `eslint-disable*` variant, so files without that
    // substring can never fire this rule.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["eslint-disable"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for &marker in MARKERS {
                if let Some(pos) = line.find(marker) {
                    let after = &line[pos + marker.len()..];
                    // Bare marker is "unlimited" — anything other than
                    // whitespace, `*/`, end-of-line is treated as a
                    // rule list and is fine.
                    let after_trim = after.trim_start();
                    if after_trim.is_empty()
                        || after_trim.starts_with("*/")
                        || after_trim.starts_with("--")
                    {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: pos + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{marker}` without a rule list disables every rule. \
                                 Name the rules explicitly."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_bare_eslint_disable_next_line() {
        let src = "// eslint-disable-next-line\nthrow err;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_block_comment() {
        let src = "/* eslint-disable */ x;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_named_rule() {
        let src = "// eslint-disable-next-line no-throw\nthrow err;";
        assert!(run(src).is_empty());
    }
}
