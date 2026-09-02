//! a11y-no-aria-hidden-on-focusable — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, bound_attr_expr, extract_elements, has_attr, is_vue_file,
};

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
            // `:aria-hidden="true"` binds the literal `true`, so it hides the
            // element exactly as the static spelling does. Any other bound
            // expression is not statically known to be `true` and is left alone.
            let is_aria_hidden = attr_value(elem.attrs, "aria-hidden") == Some("true")
                || bound_attr_expr(elem.attrs, "aria-hidden") == Some("true");
            if !is_aria_hidden {
                continue;
            }
            let is_focusable =
                FOCUSABLE_TAGS.contains(&elem.tag) || has_attr(elem.attrs, "tabindex");
            if is_focusable {
                diagnostics.push(Diagnostic::at_offset(
                    std::sync::Arc::clone(&ctx.path_arc),
                    ctx.source,
                    elem.span(),
                    "a11y-no-aria-hidden-on-focusable",
                    "`aria-hidden=\"true\"` must not be set on focusable elements.".into(),
                    Severity::Error,
                ));
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

    #[test]
    fn flags_bound_aria_hidden_pinned_to_true() {
        let source = "<template>\n  <button :aria-hidden=\"true\">X</button>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_bound_aria_hidden_with_a_dynamic_expression() {
        // `isDecorative` is not statically `true`; the element may well be
        // visible to assistive tech at runtime.
        let source = "<template>\n  <button :aria-hidden=\"isDecorative\">X</button>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_a_data_attribute_ending_in_aria_hidden() {
        let source = "<template>\n  <button data-aria-hidden=\"true\">X</button>\n</template>";
        assert!(run(source).is_empty());
    }
}
