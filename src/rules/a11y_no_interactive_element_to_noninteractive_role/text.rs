//! a11y-no-interactive-element-to-noninteractive-role — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "a", "input", "select", "textarea"];
const NON_INTERACTIVE_ROLES: &[&str] = &[
    "article", "banner", "complementary", "contentinfo", "document",
    "img", "list", "listitem", "note", "presentation", "none", "heading",
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
            if !INTERACTIVE_ELEMENTS.contains(&elem.tag) {
                continue;
            }
            if let Some(role) = attr_value(elem.attrs, "role")
                && NON_INTERACTIVE_ROLES.contains(&role)
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-no-interactive-element-to-noninteractive-role".into(),
                    message: format!("Interactive element should not have non-interactive `role=\"{role}\"`."),
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
        let source = "<template>\n  <button role=\"article\">Click</button>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_interactive_role() {
        let source = "<template>\n  <button role=\"button\">Click</button>\n</template>";
        assert!(run(source).is_empty());
    }
}
