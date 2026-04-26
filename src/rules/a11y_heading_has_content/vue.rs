//! a11y-heading-has-content — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_text_content, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !matches!(elem.tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                continue;
            }
            if elem.self_closing || !has_text_content(ctx.source, elem.line - 1, elem.tag) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-heading-has-content".into(),
                    message: format!("`<{}>` is empty and has no content.", elem.tag),
                    severity: Severity::Error,
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <h1></h1>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_heading_with_content() {
        let source = "<template>\n  <h1>Welcome</h1>\n</template>";
        assert!(run(source).is_empty());
    }
}
