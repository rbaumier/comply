//! html-no-obsolete-tags — Vue/HTML text backend.
//!
//! Scans the `<template>` block for obsolete presentational HTML tags
//! (`<center>`, `<font>`, `<marquee>`, `<blink>`, `<strike>`, `<big>`, `<tt>`)
//! and for obsolete presentational attributes (`align`, `bgcolor`, `border`).
//! `border` is only flagged when used on an element other than `<table>`,
//! where it retains its historical meaning in HTML email and legacy markup.
//! Values that are UnoCSS / Windi CSS attributify-mode utility shorthands
//! (e.g. `border="r base"`, `border="~ rounded"`) are exempt, since they are
//! utility classes rather than the presentational HTML attribute.
//! Obsolete attributes are checked only on native HTML elements; on a custom
//! Vue component (`<UPageSection>`) or custom element (`<my-card>`) the same
//! name is a modern component prop, so those elements are exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, collect_attr_names, extract_elements, is_custom_component_tag, is_vue_file,
};

const OBSOLETE_TAGS: &[&str] = &["center", "font", "marquee", "blink", "strike", "big", "tt"];

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
                    message: format!("Obsolete HTML tag `<{tag_lower}>`. Use CSS instead."),
                    severity: Severity::Error,
                    span: None,
                });
            }

            // Obsolete presentational attributes (`align`, `border`, `bgcolor`)
            // carry their legacy HTML meaning only on native elements. On a
            // custom Vue component (`<UPageSection>`) or custom element
            // (`<my-card>`) the same name is a modern component prop, never the
            // obsolete HTML attribute. Tested against `elem.tag` (not `tag_lower`)
            // so the PascalCase / hyphen casing is preserved.
            if !is_custom_component_tag(elem.tag) {
                for name in collect_attr_names(elem.attrs) {
                    let name_lower = name.to_ascii_lowercase();
                    if !OBSOLETE_ATTRS.contains(&name_lower.as_str()) {
                        continue;
                    }
                    // `border` on `<table>` is its traditional, still-understood use.
                    if name_lower == "border" && tag_lower == "table" {
                        continue;
                    }
                    if let Some(value) = attr_value(elem.attrs, name)
                        && is_utility_shorthand_value(&name_lower, value)
                    {
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
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

/// A UnoCSS / Windi CSS attributify-mode value rather than a genuine
/// presentational HTML attribute value.
///
/// Attributify mode writes utility classes as attribute values: `border="r base"`,
/// `"b 2 solid red-500"`, or the bare group marker `"~"`. A genuine obsolete
/// presentational value is a single atomic token — an integer pixel count for
/// `border` (`border="1"`), or a keyword/color for `align`/`bgcolor`
/// (`align="center"`, `bgcolor="#fff"`). So:
///   - any multi-token (whitespace-separated) value is attributify, and
///   - a single-token `border` value that is not a plain integer is a utility
///     (`border` is the only obsolete attribute that collides with attributify).
fn is_utility_shorthand_value(attr_lower: &str, value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return false;
    }
    if v.split_whitespace().count() > 1 {
        return true;
    }
    attr_lower == "border" && !v.chars().all(|c| c.is_ascii_digit())
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
    fn allows_align_on_pascalcase_component() {
        let source = "<template>\n  <UPageSection align=\"center\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_border_integer_on_pascalcase_component() {
        let source = "<template>\n  <MyCard border=\"1\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_align_on_custom_element() {
        let source = "<template>\n  <my-card align=\"center\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_border_bool_prop_on_element_plus_table() {
        // Element Plus: `border` is a documented Boolean prop of `<el-table>`,
        // not the obsolete HTML attribute — it enables cell borders in the
        // component's rendering. (#4803)
        let source = "<template>\n  <el-table :data=\"tableData\" border>\n  </el-table>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_border_bool_prop_on_element_plus_descriptions() {
        let source = "<template>\n  <el-descriptions :column=\"2\" border>\n  </el-descriptions>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_unocss_border_multi_token() {
        let source = "<template>\n  <div border=\"r base\">hi</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_unocss_border_group_marker() {
        let source = "<template>\n  <div border=\"~ rounded\"></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_unocss_border_single_non_integer() {
        let source = "<template>\n  <div border=\"rounded\"></div>\n</template>";
        assert!(run(source).is_empty());
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
