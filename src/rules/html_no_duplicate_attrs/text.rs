//! html-no-duplicate-attrs — Vue/HTML text backend.
//!
//! Scans each opening/self-closing tag inside the `<template>` block and
//! reports any attribute name that appears more than once on the same
//! element. Duplicates across different elements are fine — this rule is
//! scoped per tag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{collect_attr_names, extract_elements, is_vue_file};

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
            let mut seen: Vec<&str> = Vec::new();
            let mut reported: Vec<&str> = Vec::new();
            for name in names {
                if seen.contains(&name) {
                    if reported.contains(&name) {
                        continue;
                    }
                    reported.push(name);
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "html-no-duplicate-attrs".into(),
                        message: format!(
                            "Duplicate attribute `{name}` on `<{tag}>`.",
                            tag = elem.tag
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                } else {
                    seen.push(name);
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
    fn flags_duplicate_class() {
        let source =
            "<template>\n  <div class=\"a\" class=\"b\"></div>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("class"));
    }

    #[test]
    fn allows_unique_attrs() {
        let source =
            "<template>\n  <div class=\"a\" id=\"x\" role=\"nav\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn same_attr_on_different_elements_is_ok() {
        let source =
            "<template>\n  <div class=\"a\"></div>\n  <span class=\"a\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_duplicate_on_self_closing_tag() {
        let source = "<template>\n  <img src=\"a.png\" src=\"b.png\" />\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("src"));
    }

    #[test]
    fn reports_each_duplicate_name_once() {
        // `class` appears 3 times — only one diagnostic emitted for that name.
        let source =
            "<template>\n  <div class=\"a\" class=\"b\" class=\"c\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn ignores_non_vue_file() {
        let source =
            "<template>\n  <div class=\"a\" class=\"b\"></div>\n</template>";
        let diags = Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source));
        assert!(diags.is_empty());
    }
}
