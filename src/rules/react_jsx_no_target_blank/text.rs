//! react-jsx-no-target-blank text backend.
//!
//! Flags `target="_blank"` on elements that don't also have
//! `rel="noreferrer"` or `rel="noopener noreferrer"`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn has_safe_rel(element_text: &str) -> bool {
    let lower = element_text.to_ascii_lowercase();
    // Check for rel containing "noreferrer"
    if let Some(pos) = lower.find("rel=") {
        let after = &lower[pos + 4..];
        // Handle rel="..." or rel={'...'}
        let content = if let Some(rest) = after.strip_prefix('"') {
            rest.split('"').next().unwrap_or("")
        } else if let Some(rest) = after.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or("")
        } else if let Some(rest) = after.strip_prefix('{') {
            let inner = rest.split('}').next().unwrap_or("");
            inner.trim_matches(|c: char| c == '\'' || c == '"' || c.is_whitespace())
        } else {
            ""
        };
        return content.contains("noreferrer");
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Look for target="_blank"
            if trimmed.contains("target=\"_blank\"")
                || trimmed.contains("target='_blank'")
                || trimmed.contains("target={\"_blank\"}")
                || trimmed.contains("target={'_blank'}")
            {
                // Gather the element text (may span multiple lines)
                let mut element = String::new();
                let start = i.saturating_sub(3);
                for scan_line in lines.iter().take(lines.len().min(i + 5)).skip(start) {
                    element.push_str(scan_line);
                    element.push(' ');
                }

                if !has_safe_rel(&element) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: "react-jsx-no-target-blank".into(),
                        message: "`target=\"_blank\"` without `rel=\"noreferrer\"` \
                                  allows the opened page to access `window.opener`. \
                                  Add `rel=\"noreferrer\"`."
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
    fn flags_target_blank_without_rel() {
        let src = r#"<a href="https://example.com" target="_blank">link</a>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_target_blank_with_noreferrer() {
        let src = r#"<a href="https://example.com" target="_blank" rel="noreferrer">link</a>"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_target_blank_with_noopener_noreferrer() {
        let src = r#"<a href="https://example.com" target="_blank" rel="noopener noreferrer">link</a>"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_target_blank() {
        let src = r#"<a href="https://example.com">link</a>"#;
        assert!(run(src).is_empty());
    }
}
