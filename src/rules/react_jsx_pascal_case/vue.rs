//! react-jsx-pascal-case — Vue text backend.
//!
//! Component names in Vue templates should be PascalCase.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

/// HTML built-in elements — these are allowed in lowercase.
fn is_html_builtin(tag: &str) -> bool {
    // If it contains a hyphen, it's a web component (valid).
    // If it starts with lowercase and is all lowercase, it's likely HTML.
    tag.contains('-') || tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// Check if a tag name is PascalCase (starts with uppercase letter).
fn is_pascal_case(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // Skip HTML built-in tags and web components.
            if is_html_builtin(elem.tag) {
                continue;
            }
            // Non-HTML, non-PascalCase component name.
            if !is_pascal_case(elem.tag) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "react-jsx-pascal-case".into(),
                    message: format!(
                        "Component `<{}>` should use PascalCase.",
                        elem.tag
                    ),
                    severity: Severity::Warning,
                    span: None,
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
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn allows_pascal_case() {
        let src = "<template>\n  <MyComponent />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_html_builtin() {
        let src = "<template>\n  <div></div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<myComponent />"));
        assert!(d.is_empty());
    }
}
