//! a11y-control-has-associated-label — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, has_attr, has_text_content, is_vue_file};

const INTERACTIVE: &[&str] = &["button", "input", "select", "textarea"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !INTERACTIVE.contains(&elem.tag) {
                continue;
            }
            // <input type="hidden"> is exempt
            if elem.tag == "input"
                && attr_value(elem.attrs, "type") == Some("hidden")
            {
                continue;
            }
            if has_attr(elem.attrs, "aria-label") || has_attr(elem.attrs, "aria-labelledby") {
                continue;
            }
            // Buttons with text content are OK
            if elem.tag == "button" && !elem.self_closing
                && has_text_content(ctx.source, elem.line - 1, "button")
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: elem.line,
                column: 1,
                rule_id: "a11y-control-has-associated-label".into(),
                message: "Interactive element is missing an accessible label (`aria-label` or `aria-labelledby`).".into(),
                severity: Severity::Warning,
                span: None,
            });
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
        let source = "<template>\n  <input />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_aria_label() {
        let source = "<template>\n  <input aria-label=\"Name\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_hidden_input() {
        let source = "<template>\n  <input type=\"hidden\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_button_with_text() {
        let source = "<template>\n  <button>Submit</button>\n</template>";
        assert!(run(source).is_empty());
    }
}
