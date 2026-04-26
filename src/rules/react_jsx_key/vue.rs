//! react-jsx-key — Vue text backend.
//!
//! Flags `v-for` directives on elements that lack a `:key` binding.

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
            let has_v_for = has_attr(elem.attrs, "v-for");
            if !has_v_for {
                continue;
            }
            let has_key = has_attr(elem.attrs, ":key")
                || has_attr(elem.attrs, "v-bind:key");
            if !has_key {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "react-jsx-key".into(),
                    message: "Element with `v-for` is missing a `:key` binding — \
                              Vue needs stable keys to reconcile lists."
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
    fn flags_v_for_without_key() {
        let source = "<template>\n  <li v-for=\"item in items\">{{ item }}</li>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_v_for_with_key() {
        let source = "<template>\n  <li v-for=\"item in items\" :key=\"item.id\">{{ item }}</li>\n</template>";
        assert!(run(source).is_empty());
    }
}
