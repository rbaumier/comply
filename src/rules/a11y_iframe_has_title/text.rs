//! a11y-iframe-has-title — Vue text backend.

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
            if elem.tag == "iframe" && !has_attr(elem.attrs, "title") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-iframe-has-title".into(),
                    message: "`<iframe>` is missing a `title` attribute.".into(),
                    severity: Severity::Error,
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
    fn allows_iframe_with_title() {
        let source = "<template>\n  <iframe src=\"https://example.com\" title=\"Example\"></iframe>\n</template>";
        assert!(run(source).is_empty());
    }
}
