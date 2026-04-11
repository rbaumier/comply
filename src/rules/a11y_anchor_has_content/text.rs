//! a11y-anchor-has-content — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_attr, has_text_content, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if elem.tag != "a" {
                continue;
            }
            if has_attr(elem.attrs, "aria-label") || has_attr(elem.attrs, "aria-labelledby") {
                continue;
            }
            if elem.self_closing || !has_text_content(ctx.source, elem.line - 1, "a") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-anchor-has-content".into(),
                    message: "Anchor has no content — screen readers cannot announce it.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <a href=\"/home\"></a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_anchor_with_content() {
        let source = "<template>\n  <a href=\"/home\">Home</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_anchor_with_aria_label() {
        let source = "<template>\n  <a href=\"/home\" aria-label=\"Home\"></a>\n</template>";
        assert!(run(source).is_empty());
    }
}
