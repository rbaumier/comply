//! a11y-no-noninteractive-element-to-interactive-role — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

const NON_INTERACTIVE: &[&str] = &[
    "div", "span", "p", "section", "article", "header", "footer",
];
const INTERACTIVE_ROLES: &[&str] = &[
    "button", "link", "checkbox", "radio", "tab", "switch",
    "menuitem", "option", "textbox", "combobox", "searchbox",
    "spinbutton", "slider",
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
            if let Some(role) = attr_value(elem.attrs, "role")
                && INTERACTIVE_ROLES.contains(&role)
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-no-noninteractive-element-to-interactive-role".into(),
                    message: format!("Non-interactive element should not have interactive `role=\"{role}\"`."),
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
        let source = "<template>\n  <div role=\"button\">Click</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_noninteractive_role() {
        let source = "<template>\n  <div role=\"presentation\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
