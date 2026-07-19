//! a11y-label-has-associated-control — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    extract_elements, has_attr, is_custom_component_tag, is_vue_file, VueElement,
};

#[derive(Debug)]
pub struct Check;

/// True if `elem` is (or likely wraps) a labelable form control. Mirrors the
/// JSX backend's `label_wraps_form_control` set: native form controls plus any
/// custom component (PascalCase or hyphenated), which is the deliberate
/// implicit-association pattern for custom radio/checkbox/switch widgets.
fn is_form_control(elem: &VueElement) -> bool {
    matches!(elem.tag, "input" | "select" | "textarea" | "button")
        || is_custom_component_tag(elem.tag)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let elements = extract_elements(ctx.source);
        let mut diagnostics = Vec::new();
        for elem in &elements {
            if elem.tag != "label" {
                continue;
            }
            if has_attr(elem.attrs, "for") {
                continue;
            }
            // Implicit association: a `<label>` that wraps a form control needs
            // no `for` (HTML5), mirroring the JSX backend's descendant-control
            // check (issue #6465). Compute the `<label> … </label>` inner byte
            // span and exempt the label if any form control's opening tag falls
            // inside it. A missing close tag extends the span to end of source so
            // an unterminated wrapper still exempts; a self-closing `<label/>`
            // wraps nothing and is left to flag.
            if !elem.self_closing {
                let close = ctx.source[elem.open_end..]
                    .find("</label")
                    .map_or(ctx.source.len(), |rel| elem.open_end + rel);
                if elements.iter().any(|c| {
                    is_form_control(c) && c.open_end > elem.open_end && c.open_end <= close
                }) {
                    continue;
                }
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: elem.line,
                column: 1,
                rule_id: "a11y-label-has-associated-control".into(),
                message: "`<label>` is missing `for` — associate it with a form control."
                    .into(),
                severity: Severity::Error,
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
        let source = "<template>\n  <label>Name</label>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_label_with_for() {
        let source = "<template>\n  <label for=\"name-input\">Name</label>\n</template>";
        assert!(run(source).is_empty());
    }

    // Regression #6465: implicit association — a `<label>` wrapping a radio
    // `<input>` (the nuxt/devtools NRadio.vue pattern) needs no `for`.
    #[test]
    fn allows_label_wrapping_radio_input() {
        let source = "<template>\n  <label class=\"n-radio\" :checked=\"model === value || null\">\n    <input v-model=\"model\" type=\"radio\" :value=\"value\" :name=\"name\" />\n    <span><slot /></span>\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_label_wrapping_checkbox_input() {
        let source = "<template>\n  <label>\n    <input type=\"checkbox\" v-model=\"checked\" />\n    Accept\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    // Parity set beyond `input`: a wrapped `<select>` is also a labelable
    // control, so the label is associated implicitly.
    #[test]
    fn allows_label_wrapping_select() {
        let source = "<template>\n  <label>\n    Color\n    <select v-model=\"color\"><option>red</option></select>\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    // A deeper descendant (wrapped in an intermediate element) is exempt too,
    // matching the JSX backend's recursive descendant walk.
    #[test]
    fn allows_label_wrapping_deep_nested_input() {
        let source = "<template>\n  <label>\n    <span class=\"box\">\n      <input type=\"radio\" :value=\"v\" />\n    </span>\n    <slot />\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    // A `<label>` wrapping a custom component is the implicit-association
    // pattern for widget primitives — exempt, mirroring the JSX backend's
    // capitalized-component descendant rule.
    #[test]
    fn allows_label_wrapping_custom_component() {
        let source = "<template>\n  <label>\n    <BaseRadio :value=\"value\" />\n    <span>Label</span>\n  </label>\n</template>";
        assert!(run(source).is_empty());
    }

    // Guard: a `<label>` with no `for` and no wrapped control still flags, even
    // when a separate `<input>` sibling follows the closed label.
    #[test]
    fn flags_label_with_sibling_input() {
        let source =
            "<template>\n  <label>Name</label>\n  <input id=\"name\" type=\"text\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    // Guard: a self-closing `<label/>` wraps nothing and still flags.
    #[test]
    fn flags_self_closing_label() {
        let source = "<template>\n  <label class=\"x\" />\n  <input id=\"y\" type=\"text\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
