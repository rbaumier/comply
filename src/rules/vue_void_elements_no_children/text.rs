//! vue-void-elements-no-children — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_text_content, is_vue_file};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen",
    "link", "meta", "param", "source", "track", "wbr",
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
            if !VOID_ELEMENTS.contains(&elem.tag) {
                continue;
            }
            // If not self-closing and has text content, flag it
            if !elem.self_closing && has_text_content(ctx.source, elem.line - 1, elem.tag) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-void-elements-no-children".into(),
                    message: format!("`<{}>` is a void element and cannot have children.", elem.tag),
                    severity: Severity::Error,
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
        let source = "<template>\n  <br>text</br>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_self_closing_void() {
        let source = "<template>\n  <br />\n</template>";
        assert!(run(source).is_empty());
    }
}
