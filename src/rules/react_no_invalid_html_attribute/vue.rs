//! react-no-invalid-html-attribute — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

/// Valid `rel` values for `<a>` elements.
const VALID_A_RELS: &[&str] = &[
    "alternate", "author", "bookmark", "external", "help", "license",
    "next", "nofollow", "noopener", "noreferrer", "opener", "prev",
    "search", "tag", "ugc", "sponsored",
];

/// Valid `rel` values for `<link>` elements.
const VALID_LINK_RELS: &[&str] = &[
    "alternate", "author", "canonical", "dns-prefetch", "help", "icon",
    "license", "manifest", "modulepreload", "next", "pingback",
    "preconnect", "prefetch", "preload", "prerender", "prev", "search",
    "shortlink", "stylesheet", "apple-touch-icon",
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
            let valid_rels = match elem.tag {
                "a" => VALID_A_RELS,
                "link" => VALID_LINK_RELS,
                _ => continue,
            };
            let Some(rel_val) = attr_value(elem.attrs, "rel") else {
                continue;
            };
            for token in rel_val.split_whitespace() {
                if !valid_rels.contains(&token) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "react-no-invalid-html-attribute".into(),
                        message: format!(
                            "Invalid `rel` value `{token}` on `<{}>`.",
                            elem.tag,
                        ),
                        severity: Severity::Warning,
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
    fn flags_invalid_rel() {
        let source = "<template>\n  <a rel=\"invalid\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_valid_rel() {
        let source = "<template>\n  <a rel=\"noopener noreferrer\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }
}
