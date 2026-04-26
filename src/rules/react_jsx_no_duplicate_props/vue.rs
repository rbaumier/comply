//! react-jsx-no-duplicate-props — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{collect_attr_names, extract_elements, is_vue_file};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            let names = collect_attr_names(elem.attrs);
            let mut seen = HashSet::new();
            for name in &names {
                if !seen.insert(*name) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "react-jsx-no-duplicate-props".into(),
                        message: format!(
                            "Duplicate attribute `{name}` — the last value silently wins."
                        ),
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
        let source = "<template>\n  <div class=\"a\" class=\"b\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_unique_attrs() {
        let source = "<template>\n  <div class=\"a\" id=\"b\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
