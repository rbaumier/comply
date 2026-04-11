//! react-style-prop-object text backend.
//!
//! Flags `style="..."` in JSX — React expects the `style` prop to be an
//! object, not a CSS string.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            // Skip non-JSX lines
            if !trimmed.contains("style=\"") && !trimmed.contains("style='") {
                continue;
            }
            // Must be in a JSX context — line should contain `<` or be
            // inside a JSX element (starts with a prop-like pattern).
            if (trimmed.contains('<')
                || trimmed.starts_with("style=")
                || trimmed.contains(" style=\"")
                || trimmed.contains(" style='"))
                && (trimmed.contains("style=\"") || trimmed.contains("style='"))
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "react-style-prop-object".into(),
                    message: "The `style` prop expects a JavaScript object, \
                              not a CSS string. Use `style={{ ... }}` instead."
                        .into(),
                    severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("App.tsx"), source))
    }

    #[test]
    fn flags_string_style() {
        let src = r#"<div style="color: red">hello</div>"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_quote_style() {
        let src = "<div style='color: red'>hello</div>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_style() {
        let src = r#"<div style={{ color: "red" }}>hello</div>"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_style() {
        let src = "<div style={myStyles}>hello</div>";
        assert!(run(src).is_empty());
    }
}
