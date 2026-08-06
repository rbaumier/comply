//! a11y-no-autofocus — Vue text backend.

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
            if has_attr(elem.attrs, "autofocus") {
                diagnostics.push(Diagnostic::at_offset(
                    std::sync::Arc::clone(&ctx.path_arc),
                    ctx.source,
                    elem.attr_span("autofocus"),
                    "a11y-no-autofocus",
                    "Avoid `autofocus` — it is disorienting for screen reader users."
                        .into(),
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
        let source = "<template>\n  <input autofocus />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_no_autofocus() {
        let source = "<template>\n  <input type=\"text\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn anchors_on_the_autofocus_attribute() {
        // The oxc backend anchors on the offending attribute, not the element,
        // so the Vue backend names the same construct: the reported bytes are
        // `autofocus` itself.
        let source = "<template>\n  <input type=\"text\" autofocus />\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        let (start, len) = diags[0].span.expect("the diagnostic carries a span");
        assert_eq!(&source[start..start + len], "autofocus");
        assert_eq!((diags[0].line, diags[0].column), (2, 22));
    }

    #[test]
    fn anchors_on_the_shorthand_binding_of_autofocus() {
        // `:autofocus` is the `v-bind` shorthand of the same attribute, so it
        // is still named by the diagnostic.
        let source = "<template>\n  <input :autofocus=\"shouldFocus\" />\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        let (start, len) = diags[0].span.expect("the diagnostic carries a span");
        assert_eq!(&source[start..start + len], ":autofocus");
        assert_eq!((diags[0].line, diags[0].column), (2, 10));
    }
}
