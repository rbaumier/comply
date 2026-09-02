//! a11y-tabindex-no-positive — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, bound_attr_expr, extract_elements, is_vue_file,
};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // A bound `:tabindex="1"` is an expression, but an integer literal
            // in it is as statically known as a plain attribute value.
            if let Some(val) =
                attr_value(elem.attrs, "tabindex").or_else(|| bound_attr_expr(elem.attrs, "tabindex"))
                && let Ok(n) = val.trim().parse::<i32>()
                && n > 0
            {
                diagnostics.push(Diagnostic::at_offset(
                    std::sync::Arc::clone(&ctx.path_arc),
                    ctx.source,
                    elem.attr_span("tabindex"),
                    "a11y-tabindex-no-positive",
                    "`tabindex` must not be positive — use `0` or `-1` only.".into(),
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
        let source = "<template>\n  <div tabindex=\"5\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_zero() {
        let source = "<template>\n  <div tabindex=\"0\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    /// A bound attribute holding an integer literal is as statically known as a
    /// plain one. Issue #8424 split the static and bound readers apart, so this
    /// shape needs the bound reader by name.
    #[test]
    fn flags_a_bound_positive_tabindex() {
        let source = "<template>\n  <div :tabindex=\"1\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    /// A bound expression that is not an integer literal has no statically
    /// known value and stays silent.
    #[test]
    fn allows_a_bound_tabindex_expression() {
        let source = "<template>\n  <div :tabindex=\"level\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_negative_one() {
        let source = "<template>\n  <div tabindex=\"-1\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
