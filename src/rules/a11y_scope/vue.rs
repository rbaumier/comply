//! a11y-scope — Vue text backend.

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
            if elem.tag == "th" {
                continue;
            }
            if has_attr(elem.attrs, "scope") {
                diagnostics.push(Diagnostic::at_offset(
                    std::sync::Arc::clone(&ctx.path_arc),
                    ctx.source,
                    elem.attr_span("scope"),
                    "a11y-scope",
                    "`scope` attribute should only be used on `<th>` elements.".into(),
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
        let source = "<template>\n  <td scope=\"row\">Name</td>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_scope_on_th() {
        let source = "<template>\n  <th scope=\"col\">Name</th>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_slot_scope() {
        // #8424: `slot-scope` is Vue 2's scoped-slot syntax, not the HTML
        // `scope` attribute. 69 occurrences on element-plus.
        let source = "<template>\n  <td slot-scope=\"props\">x</td>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_bound_scope_on_td() {
        // `:scope` still puts a `scope` attribute on a non-`<th>` element.
        let source = "<template>\n  <td :scope=\"s\">x</td>\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
