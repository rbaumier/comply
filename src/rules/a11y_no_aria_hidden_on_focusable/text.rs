//! a11y-no-aria-hidden-on-focusable — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, has_attr, is_vue_file};

const FOCUSABLE_TAGS: &[&str] = &["button", "a", "input", "select", "textarea"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            let is_aria_hidden = attr_value(elem.attrs, "aria-hidden") == Some("true");
            if !is_aria_hidden {
                continue;
            }
            let is_focusable = FOCUSABLE_TAGS.contains(&elem.tag) || has_attr(elem.attrs, "tabindex");
            if is_focusable {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-no-aria-hidden-on-focusable".into(),
                    message: "`aria-hidden=\"true\"` must not be set on focusable elements.".into(),
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
        let source = "<template>\n  <button aria-hidden=\"true\">X</button>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_aria_hidden_on_div() {
        let source = "<template>\n  <div aria-hidden=\"true\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
