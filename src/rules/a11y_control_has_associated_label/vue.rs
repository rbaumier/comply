//! a11y-control-has-associated-label — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, extract_elements, has_attr, has_text_content, is_vue_file,
};

const INTERACTIVE: &[&str] = &["button", "input", "select", "textarea"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let elements = extract_elements(ctx.source);
        // Byte spans of every `<label> … </label>` in the template. A form
        // control nested inside one is labeled implicitly by HTML5, so it needs
        // no `aria-label`/`aria-labelledby` — mirroring the JSX backend's
        // ancestor-`<label>` check (issue #2001). A missing close tag extends
        // the span to end of source so an unterminated wrapper still exempts.
        let label_spans: Vec<(usize, usize)> = elements
            .iter()
            .filter(|e| e.tag == "label" && !e.self_closing)
            .map(|e| {
                let close = ctx.source[e.open_end..]
                    .find("</label")
                    .map_or(ctx.source.len(), |rel| e.open_end + rel);
                (e.open_end, close)
            })
            .collect();
        let mut diagnostics = Vec::new();
        for elem in &elements {
            if !INTERACTIVE.contains(&elem.tag) {
                continue;
            }
            // <input type="hidden"> is exempt
            if elem.tag == "input" && attr_value(elem.attrs, "type") == Some("hidden") {
                continue;
            }
            if has_attr(elem.attrs, "aria-label") || has_attr(elem.attrs, "aria-labelledby") {
                continue;
            }
            // Implicit label association: control nested inside a `<label>`.
            if label_spans
                .iter()
                .any(|&(start, end)| elem.open_end > start && elem.open_end <= end)
            {
                continue;
            }
            // Buttons with text content are OK
            if elem.tag == "button"
                && !elem.self_closing
                && has_text_content(ctx.source, elem.line - 1, "button")
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
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

    // Regression #6474: implicit label association — a control wrapped in a
    // `<label>` is labeled by it and needs no `aria-label`.
    #[test]
    fn allows_radio_input_wrapped_in_label() {
        let source = "<template>\n  <label class=\"n-radio\">\n    <input v-model=\"model\" type=\"radio\" :value=\"value\" />\n    <span><slot /></span>\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_checkbox_input_wrapped_in_label() {
        let source = "<template>\n  <label>\n    <input type=\"checkbox\" v-model=\"checked\" />\n    Accept\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    // A deeper descendant of the `<label>` (wrapped in an intermediate element)
    // is exempt too, matching the JSX backend's descendant behavior.
    #[test]
    fn allows_input_deep_descendant_of_label() {
        let source = "<template>\n  <label>\n    <span class=\"box\">\n      <input type=\"radio\" :value=\"v\" />\n    </span>\n    <slot />\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    // Guard: a bare control with no wrapping label still flags.
    #[test]
    fn still_flags_input_without_wrapping_label() {
        let source = "<template>\n  <input type=\"text\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    // Guard: a sibling `<label>` (already closed) is not an ancestor, so the
    // control is still flagged.
    #[test]
    fn still_flags_input_after_closed_label() {
        let source = "<template>\n  <label for=\"x\">Name</label>\n  <input id=\"y\" type=\"text\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
