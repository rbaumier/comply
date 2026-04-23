//! html-no-non-scalable-viewport — scans HTML/Vue text for viewport meta tags
//! that disable user scaling via `user-scalable=no` or `user-scalable=0`.
//! Disabling zoom breaks accessibility for users who need to enlarge content.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const NEEDLES: &[&str] = &[
    "user-scalable=no",
    "user-scalable=\"no\"",
    "user-scalable='no'",
    "user-scalable=0",
    "user-scalable=\"0\"",
    "user-scalable='0'",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            for needle in NEEDLES {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(needle) {
                    let match_start = search_from + rel;
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: match_start + 1,
                        rule_id: super::META.id.into(),
                        message: "Viewport meta tag disables user scaling. Allow users to zoom."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    search_from = match_start + needle.len();
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
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    #[test]
    fn flags_user_scalable_no() {
        let src = "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, user-scalable=no\">";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_user_scalable_zero() {
        let src = "<meta name=\"viewport\" content=\"width=device-width, user-scalable=0\">";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_user_scalable_quoted_no() {
        let src = "<meta name=\"viewport\" content='width=device-width, user-scalable=\"no\"'>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_user_scalable_yes() {
        let src = "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, user-scalable=yes\">";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_missing_user_scalable() {
        let src = "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_viewport_at_all() {
        let src = "<template><div>hello</div></template>";
        assert!(run(src).is_empty());
    }
}
