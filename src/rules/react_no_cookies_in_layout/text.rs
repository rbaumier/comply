//! react-no-cookies-in-layout text backend.
//!
//! Detects `cookies()` or `headers()` calls in files named `layout.*`.
//! In Next.js App Router, these functions opt the route into dynamic
//! rendering. When called from a layout, the blast radius is the entire
//! route segment: every child page becomes dynamic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DYNAMIC_CALLS: &[&str] = &["cookies()", "headers()"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only fire on files named `layout.tsx`, `layout.ts`, `layout.js`, etc.
        let file_stem = ctx.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if file_stem != "layout" {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for &call in DYNAMIC_CALLS {
                if line.contains(call) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-no-cookies-in-layout".into(),
                        message: format!(
                            "`{call}` in a layout file forces EVERY child page to \
                             be dynamically rendered. Move it to the individual page \
                             that needs it."
                        ),
                        severity: Severity::Error,
                    });
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

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_cookies_in_layout() {
        assert_eq!(
            run("app/layout.tsx", "const c = cookies();\nexport default function Layout() {}").len(),
            1
        );
    }

    #[test]
    fn flags_headers_in_layout() {
        assert_eq!(
            run("app/layout.ts", "const h = headers();").len(),
            1
        );
    }

    #[test]
    fn allows_cookies_in_page() {
        assert!(run("app/page.tsx", "const c = cookies();").is_empty());
    }

    #[test]
    fn allows_layout_without_dynamic_calls() {
        assert!(run("app/layout.tsx", "export default function Layout() {}").is_empty());
    }
}
