//! a11y-no-noninteractive-element-interactions — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_attr, is_vue_file};

const NON_INTERACTIVE: &[&str] = &[
    "div", "span", "p", "section", "article", "header", "footer", "main", "aside", "nav",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !NON_INTERACTIVE.contains(&elem.tag) {
                continue;
            }
            let has_handler = has_attr(elem.attrs, "@click")
                || has_attr(elem.attrs, "@keydown")
                || has_attr(elem.attrs, "v-on:click")
                || has_attr(elem.attrs, "v-on:keydown");
            let has_role = has_attr(elem.attrs, "role");
            if has_handler && !has_role {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-no-noninteractive-element-interactions".into(),
                    message: format!(
                        "Non-interactive element `<{}>` has an event handler without a `role` attribute.",
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <div @click=\"handler\">Click</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_role() {
        let source =
            "<template>\n  <div @click=\"handler\" role=\"button\">Click</div>\n</template>";
        assert!(run(source).is_empty());
    }
}
