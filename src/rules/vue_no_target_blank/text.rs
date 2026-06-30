//! vue-no-target-blank — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::html_rel_helpers::rel_is_safe;
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["_blank"])
    }

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
            let has_safe_rel = attr_value(elem.attrs, "rel").is_some_and(rel_is_safe);
            if !has_safe_rel {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-no-target-blank".into(),
                    message: "`target=\"_blank\"` without `rel=\"noopener\"` (or `noreferrer`) \
                              allows the opened page to access `window.opener`. \
                              Add `rel=\"noopener\"`."
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
        let source =
            "<template>\n  <a href=\"https://example.com\" target=\"_blank\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_noreferrer() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"noreferrer\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_with_noopener() {
        // `rel="noopener"` alone severs `window.opener` (issue #6939, real snippet
        // uses a bound `:href`).
        let source = "<template>\n  <a :href=\"item.docsUrl\" target=\"_blank\" rel=\"noopener\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_with_noopener_noreferrer() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_unrelated_rel_token() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"nofollow\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_substring_trap() {
        // `notnoopener` merely contains `noopener` as a substring; it is not the token.
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"notnoopener\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
