//! a11y-no-redundant-roles — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

const REDUNDANT_PAIRS: &[(&str, &str)] = &[
    ("button", "button"), ("nav", "navigation"), ("img", "img"),
    ("input", "textbox"), ("h1", "heading"), ("h2", "heading"),
    ("h3", "heading"), ("h4", "heading"), ("h5", "heading"),
    ("h6", "heading"), ("ul", "list"), ("ol", "list"),
    ("li", "listitem"), ("table", "table"), ("form", "form"),
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if let Some(role) = attr_value(elem.attrs, "role") {
                for &(tag, redundant_role) in REDUNDANT_PAIRS {
                    if elem.tag == tag && role == redundant_role {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: elem.line,
                            column: 1,
                            rule_id: "a11y-no-redundant-roles".into(),
                            message: format!(
                                "`<{tag}>` has implicit role `{redundant_role}` — `role=\"{role}\"` is redundant."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
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
        let source = "<template>\n  <button role=\"button\">Click</button>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_different_role() {
        let source = "<template>\n  <button role=\"tab\">Tab</button>\n</template>";
        assert!(run(source).is_empty());
    }
}
