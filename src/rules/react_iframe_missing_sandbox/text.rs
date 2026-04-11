//! react-iframe-missing-sandbox text backend.
//!
//! Flags `<iframe` elements that don't have a `sandbox` attribute.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let lower = line.trim().to_ascii_lowercase();

            if lower.contains("<iframe") {
                // Gather the full element (may span multiple lines)
                let mut element = String::new();
                for scan_line in lines.iter().take(lines.len().min(i + 10)).skip(i) {
                    element.push_str(scan_line);
                    element.push(' ');
                    if scan_line.contains("/>") || scan_line.trim().ends_with('>') {
                        break;
                    }
                }

                let lower_elem = element.to_ascii_lowercase();
                if !lower_elem.contains("sandbox") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: "react-iframe-missing-sandbox".into(),
                        message: "`<iframe>` without a `sandbox` attribute can access \
                                  the parent page. Add `sandbox` to restrict its \
                                  capabilities."
                            .into(),
                        severity: Severity::Warning,
                    });
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
    fn flags_iframe_without_sandbox() {
        let src = r#"<iframe src="https://example.com" />"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_iframe_with_sandbox() {
        let src = r#"<iframe src="https://example.com" sandbox="allow-scripts" />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_iframe_with_empty_sandbox() {
        let src = r#"<iframe src="https://example.com" sandbox="" />"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_iframe() {
        let src = r#"<div src="https://example.com" />"#;
        assert!(run(src).is_empty());
    }
}
