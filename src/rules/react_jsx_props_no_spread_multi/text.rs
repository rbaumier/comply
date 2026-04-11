//! react-jsx-props-no-spread-multi — Vue text backend.
//!
//! Flags `v-bind` spread used multiple times on the same element.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // Count `v-bind=` occurrences in attrs (spread binding in Vue).
            let spread_count = elem.attrs.matches("v-bind=").count();
            if spread_count > 1 {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "react-jsx-props-no-spread-multi".into(),
                    message: "Same `v-bind` spread used multiple times on this element.".into(),
                    severity: Severity::Warning,
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
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn flags_multiple_v_bind_spread() {
        let src = "<template>\n  <div v-bind=\"a\" v-bind=\"b\"></div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_spread() {
        let src = "<template>\n  <div v-bind=\"props\"></div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(
            Path::new("f.ts"),
            "<div v-bind=\"a\" v-bind=\"b\"></div>",
        ));
        assert!(d.is_empty());
    }
}
