//! eslint-comments-require-description text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MARKERS: &[&str] = &[
    "eslint-disable-next-line",
    "eslint-disable-line",
    "eslint-disable",
    "eslint-enable",
];

impl TextCheck for Check {
    // Every marker is an `eslint-disable*` or `eslint-enable` directive; a file
    // containing neither substring can never fire this rule.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["eslint-disable", "eslint-enable"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for &marker in MARKERS {
                if let Some(pos) = line.find(marker) {
                    let after = &line[pos + marker.len()..];
                    let trailing = after.trim_end_matches("*/").trim();
                    // Justification is present after `--` or em-dash.
                    if trailing.contains(" -- ") || trailing.contains('—') {
                        break;
                    }
                    // Empty / whitespace-only after the marker — flag.
                    if trailing.is_empty()
                        || trailing
                            .split(' ')
                            .all(|t| t.is_empty() || t.starts_with('-'))
                    {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: pos + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{marker}` without a justification — add `-- <reason>` \
                                 so the next reader knows why the rule is disabled."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                    break;
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
    fn allows_with_dash_dash_reason() {
        let src = "// eslint-disable-next-line no-throw -- legacy bridge\nthrow err;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_with_em_dash_reason() {
        let src = "// eslint-disable-next-line no-throw — legacy\nthrow err;";
        assert!(run(src).is_empty());
    }
}
