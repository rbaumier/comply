//! vue-button-has-type — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, has_attr, is_vue_file};

const VALID_TYPES: &[&str] = &["button", "submit", "reset"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if elem.tag != "button" {
                continue;
            }
            let valid = if has_attr(elem.attrs, "type") {
                attr_value(elem.attrs, "type")
                    .is_none_or(|v| VALID_TYPES.contains(&v))
            } else {
                false
            };
            if !valid {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-button-has-type".into(),
                    message: "`<button>` missing an explicit `type` attribute — \
                              defaults to `submit`, which may cause unexpected \
                              form submissions."
                        .into(),
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
    fn flags_vue_button_without_type() {
        let source = "<template>\n  <button>Click</button>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_vue_button_with_type() {
        let source = "<template>\n  <button type=\"button\">Click</button>\n</template>";
        assert!(run(source).is_empty());
    }
}
