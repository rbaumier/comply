//! a11y-anchor-is-valid — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, has_attr, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if elem.tag != "a" {
                continue;
            }
            if !has_attr(elem.attrs, "href") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-anchor-is-valid".into(),
                    message: "Anchor is missing an `href` attribute.".into(),
                    severity: Severity::Error,
                    span: None,
                });
                continue;
            }
            if let Some(val) = attr_value(elem.attrs, "href") {
                if val == "#" {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-anchor-is-valid".into(),
                        message: "Anchor has `href=\"#\"` — use a `<button>` or a real URL.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                } else if val.contains("javascript:") {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-anchor-is-valid".into(),
                        message: "Anchor has `href=\"javascript:\"` — use a `<button>` or a real URL.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
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
        let source = "<template>\n  <a href=\"#\">Click</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_missing_href() {
        let source = "<template>\n  <a @click=\"handler\">Click</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_valid_href() {
        let source = "<template>\n  <a href=\"/home\">Home</a>\n</template>";
        assert!(run(source).is_empty());
    }
}
