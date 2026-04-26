//! a11y-no-static-element-interactions — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_attr, is_vue_file};

const STATIC_ELEMENTS: &[&str] = &["div", "span"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !STATIC_ELEMENTS.contains(&elem.tag) {
                continue;
            }
            let has_click = has_attr(elem.attrs, "@click")
                || has_attr(elem.attrs, "v-on:click");
            let has_role = has_attr(elem.attrs, "role");
            if has_click && !has_role {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-no-static-element-interactions".into(),
                    message: format!(
                        "Static element `<{}>` has `@click` without a `role` attribute.",
                        elem.tag
                    ),
                    severity: Severity::Warning,
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
        let source = "<template>\n  <div @click=\"handler\">Click</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_role() {
        let source = "<template>\n  <div @click=\"handler\" role=\"button\">Click</div>\n</template>";
        assert!(run(source).is_empty());
    }
}
