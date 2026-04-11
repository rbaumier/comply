//! a11y-no-noninteractive-tabindex — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

const NON_INTERACTIVE: &[&str] = &["div", "span", "p", "section"];

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
            if let Some(val) = attr_value(elem.attrs, "tabindex") {
                // tabindex="-1" is OK
                if val != "-1" {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-no-noninteractive-tabindex".into(),
                        message: format!(
                            "Non-interactive element `<{}>` should not have `tabindex`.",
                            elem.tag
                        ),
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <div tabindex=\"0\">Focusable</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_tabindex_negative_one() {
        let source = "<template>\n  <div tabindex=\"-1\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
