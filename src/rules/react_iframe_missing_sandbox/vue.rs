//! react-iframe-missing-sandbox — Vue text backend.

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
            if elem.tag == "iframe" && !has_attr(elem.attrs, "sandbox") {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "react-iframe-missing-sandbox".into(),
                    message: "`<iframe>` without a `sandbox` attribute can access \
                              the parent page. Add `sandbox` to restrict its \
                              capabilities."
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
    fn flags_vue_template() {
        let source = "<template>\n  <iframe src=\"https://example.com\"></iframe>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_sandbox() {
        let source = "<template>\n  <iframe src=\"https://example.com\" sandbox=\"allow-scripts\"></iframe>\n</template>";
        assert!(run(source).is_empty());
    }
}
