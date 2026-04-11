//! react-jsx-no-script-url text backend.
//!
//! Flags `href="javascript:..."` or `href={'javascript:...'}` in JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    "href=\"javascript:",
    "href='javascript:",
    "href={\"javascript:",
    "href={'javascript:",
    "href={`javascript:",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            for &pattern in PATTERNS {
                if lower.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "react-jsx-no-script-url".into(),
                        message: "`javascript:` URLs are an XSS vector. Use an \
                                  `onClick` handler instead."
                            .into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_javascript_href() {
        let src = r#"<a href="javascript:alert('xss')">click</a>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_javascript_href_expression() {
        let src = r#"<a href={'javascript:void(0)'}>click</a>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_javascript_href_template() {
        let src = r#"<a href={`javascript:alert(1)`}>click</a>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal_href() {
        let src = r#"<a href="https://example.com">click</a>"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_hash_href() {
        let src = r##"<a href="#">click</a>"##;
        assert!(run(src).is_empty());
    }
}
