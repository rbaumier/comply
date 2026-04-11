//! a11y-aria-activedescendant-has-tabindex — Vue text backend.

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
            if !has_attr(elem.attrs, "aria-activedescendant") {
                continue;
            }
            if !has_attr(elem.attrs, "tabindex") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-aria-activedescendant-has-tabindex".into(),
                    message: "Element with `aria-activedescendant` must have `tabIndex` to be tabbable.".into(),
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
        let source = "<template>\n  <div aria-activedescendant=\"item-1\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_tabindex() {
        let source = "<template>\n  <div aria-activedescendant=\"item-1\" tabindex=\"0\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
