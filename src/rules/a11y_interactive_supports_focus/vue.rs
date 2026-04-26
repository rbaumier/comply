//! a11y-interactive-supports-focus — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_attr, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            let has_handler = has_attr(elem.attrs, "@click")
                || has_attr(elem.attrs, "@keydown")
                || has_attr(elem.attrs, "v-on:click")
                || has_attr(elem.attrs, "v-on:keydown");
            let has_role = has_attr(elem.attrs, "role");
            let has_tabindex = has_attr(elem.attrs, "tabindex");
            if has_handler && has_role && !has_tabindex {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-interactive-supports-focus".into(),
                    message: "Element with interactive handler and `role` must have `tabindex` to be focusable.".into(),
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
        let source = "<template>\n  <div @click=\"handler\" role=\"button\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_tabindex() {
        let source = "<template>\n  <div @click=\"handler\" role=\"button\" tabindex=\"0\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
