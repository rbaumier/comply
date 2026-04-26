//! html-no-obsolete-tags — Vue/HTML text backend.
//!
//! Scans the `<template>` block for obsolete presentational HTML tags
//! (`<center>`, `<font>`, `<marquee>`, `<blink>`, `<strike>`, `<big>`, `<tt>`)
//! and for obsolete presentational attributes (`align`, `bgcolor`, `border`).
//! `border` is only flagged when used on an element other than `<table>`,
//! where it retains its historical meaning in HTML email and legacy markup.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{collect_attr_names, extract_elements, is_vue_file};

const OBSOLETE_TAGS: &[&str] = &[
    "center", "font", "marquee", "blink", "strike", "big", "tt",
];

const OBSOLETE_ATTRS: &[&str] = &["align", "bgcolor", "border"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            let tag_lower = elem.tag.to_ascii_lowercase();
            if OBSOLETE_TAGS.contains(&tag_lower.as_str()) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Obsolete HTML tag `<{tag_lower}>`. Use CSS instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            for name in collect_attr_names(elem.attrs) {
                let name_lower = name.to_ascii_lowercase();
                if !OBSOLETE_ATTRS.contains(&name_lower.as_str()) {
                    continue;
                }
                // `border` on `<table>` is its traditional, still-understood use.
                if name_lower == "border" && tag_lower == "table" {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Obsolete HTML attribute `{name_lower}` on `<{tag_lower}>`. Use CSS instead."
                    ),
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

    fn run_named(name: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(name), source))
    }

    #[test]
    fn flags_center_tag() {
        let source = "<template>\n  <center>hi</center>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_font_tag() {
        let source = "<template>\n  <font color=\"red\">x</font>\n</template>";
        let diags = run(source);
        assert!(diags.iter().any(|d| d.message.contains("<font>")));
    }

    #[test]
    fn flags_all_obsolete_tags() {
        let source = "<template>\n  <center></center>\n  <font></font>\n  <marquee></marquee>\n  <blink></blink>\n  <strike></strike>\n  <big></big>\n  <tt></tt>\n</template>";
        assert_eq!(run(source).len(), 7);
    }

    #[test]
    fn flags_align_attribute() {
        let source = "<template>\n  <div align=\"center\">hi</div>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("align"));
    }

    #[test]
    fn flags_bgcolor_attribute() {
        let source = "<template>\n  <div bgcolor=\"#fff\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_border_on_non_table() {
        let source = "<template>\n  <img src=\"x\" border=\"1\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_border_on_table() {
        let source = "<template>\n  <table border=\"1\"></table>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_modern_html() {
        let source = "<template>\n  <div class=\"text-center\">hi</div>\n  <span style=\"color: red\">x</span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_non_vue_file() {
        let source = "<template>\n  <center>hi</center>\n</template>";
        assert!(run_named("component.tsx", source).is_empty());
    }
}
