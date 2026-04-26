//! react-jsx-no-target-blank — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            let target = attr_value(elem.attrs, "target");
            if target != Some("_blank") {
                continue;
            }
            let has_safe_rel = attr_value(elem.attrs, "rel")
                .is_some_and(|v| v.to_ascii_lowercase().contains("noreferrer"));
            if !has_safe_rel {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "react-jsx-no-target-blank".into(),
                    message: "`target=\"_blank\"` without `rel=\"noreferrer\"` \
                              allows the opened page to access `window.opener`. \
                              Add `rel=\"noreferrer\"`."
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
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_noreferrer() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"noreferrer\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }
}
