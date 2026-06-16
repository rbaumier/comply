//! Port of Biome `noUnknownUnit`.
//!
//! Flags CSS dimension units that are not part of the recognized set of CSS
//! units (length, container-query length, angle, time, frequency, resolution,
//! and flex units). The unit set mirrors the one Biome's CSS parser uses to
//! distinguish a regular dimension from an unknown one, so `13pix`, `400x` (in
//! most contexts), and `1e4pz` are flagged while `1px`, `2rem`, and `90deg`
//! are accepted.
//!
//! The resolution unit `x` is special: it is only valid inside an
//! `image-set()`-family function, the `image-resolution` property, or a
//! `resolution`/`min-resolution`/`max-resolution` media feature. Anywhere else
//! `x` is reported, matching Biome.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::FxHashSet;
use std::sync::LazyLock;

/// Every CSS unit Biome's parser treats as a known dimension unit. Units are
/// compared case-insensitively, so this set is stored lowercased. Grouped to
/// mirror the `TokenSet` constants in Biome's `value/dimension.rs`.
static KNOWN_UNITS: LazyLock<FxHashSet<&'static str>> = LazyLock::new(|| {
    [
        // Length units.
        "em", "rem", "ex", "rex", "cap", "rcap", "ch", "rch", "ic", "ric", "lh", "rlh", "vw",
        "svw", "lvw", "dvw", "vh", "svh", "lvh", "dvh", "vi", "svi", "lvi", "dvi", "vb", "svb",
        "lvb", "dvb", "vmin", "svmin", "lvmin", "dvmin", "vmax", "svmax", "lvmax", "dvmax", "cm",
        "mm", "q", "in", "pc", "pt", "px", "mozmm", "rpx",
        // Container-query length units.
        "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax",
        // Angle units.
        "deg", "grad", "rad", "turn",
        // Time units.
        "s", "ms",
        // Frequency units.
        "hz", "khz",
        // Resolution units.
        "dpi", "dpcm", "dppx", "x",
        // Flex units.
        "fr",
    ]
    .into_iter()
    .collect()
});

/// Media features that permit the `x` resolution unit.
const RESOLUTION_FEATURES: &[&str] = &["resolution", "min-resolution", "max-resolution"];

/// Whether the `x` unit at `node` sits in a context where it denotes a valid
/// resolution: an `image-set()`-family function, the `image-resolution`
/// property, or a resolution media feature. Mirrors Biome's ancestor walk.
fn x_is_allowed(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        match ancestor.kind() {
            "call_expression" => {
                if let Some(name) = child_text(&ancestor, "function_name", source)
                    && name.to_ascii_lowercase().ends_with("image-set")
                {
                    return true;
                }
            }
            "declaration" => {
                if let Some(name) = child_text(&ancestor, "property_name", source)
                    && name.eq_ignore_ascii_case("image-resolution")
                {
                    return true;
                }
            }
            "feature_query" => {
                if let Some(name) = child_text(&ancestor, "feature_name", source)
                    && RESOLUTION_FEATURES.contains(&name.to_ascii_lowercase().as_str())
                {
                    return true;
                }
            }
            _ => {}
        }
        cursor = ancestor.parent();
    }
    false
}

/// Text of the first direct child of `node` whose kind is `kind`.
fn child_text<'a>(node: &tree_sitter::Node, kind: &str, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == kind)
        .and_then(|child| child.utf8_text(source).ok())
}

/// Whether the unit at `node` is the content of a `url()` function. tree-sitter
/// parses `url(13pix)` into a dimension, but a URL is an opaque token in CSS, so
/// units there are not real dimensions and must not be flagged.
fn is_in_url(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        if ancestor.kind() == "call_expression"
            && let Some(name) = child_text(&ancestor, "function_name", source)
        {
            return name.eq_ignore_ascii_case("url");
        }
        cursor = ancestor.parent();
    }
    false
}

crate::ast_check! { on ["unit"] => |node, source, ctx, diagnostics|
    let unit = node.utf8_text(source).unwrap_or_default();
    if unit.is_empty() {
        return;
    }
    // `%` is a percentage, not a dimension unit (Biome models it as a separate
    // node and never flags it), and a URL's content is an opaque token whose
    // trailing characters are not a real unit.
    if unit == "%" || is_in_url(&node, source) {
        return;
    }
    let lower = unit.to_ascii_lowercase();

    if !KNOWN_UNITS.contains(lower.as_str()) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Unexpected unknown unit `{unit}`."),
            Severity::Warning,
        ));
        return;
    }

    // `x` is a known resolution unit but only valid in a handful of contexts.
    if lower == "x" && !x_is_allowed(&node, source) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Unexpected unknown unit `{unit}`."),
            Severity::Warning,
        ));
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    // --- Biome `valid.css` fixtures: must not fire. ---

    #[test]
    fn allows_unitless_and_known_units() {
        for src in [
            "a { line-height: 1; }",
            "a { color: #000; }",
            "a { font-size: 100%; }",
            "a { margin: 1em; }",
            "a { margin: 1Em; }",
            "a { margin: 1EM; }",
            "a { margin: 1ex; }",
            "a { margin: 1%; }",
            "a { margin: 1px; }",
            "a { margin: 1cm; }",
            "a { margin: 1mm; }",
            "a { margin: 1in; }",
            "a { margin: 1pt; }",
            "a { margin: 1pc; }",
            "a { margin: 1ch; }",
            "a { margin: 1rem; }",
            "a { margin: 1vh; }",
            "a { margin: 1vw; }",
            "a { margin: 1vmin; }",
            "a { margin: 1vmax; }",
            "a { font-size: .5rem; }",
            "a { font-size: 0.5rem; }",
            "a { margin: 1vmin 1vmax; }",
            "a { margin: 0 10em 5rem 2in; }",
            "a { background-position: top right, 1em 5vh; }",
            "a { top: calc(10em - 3em); }",
            "a { top: calc(10px*2); }",
            "a { top: calc(2*10px); }",
            "a { transition-delay: 3s; }",
            "a { transition-delay: 300ms; }",
            "a { transform: rotate(90deg); }",
            "a { transform: rotate(100grad); }",
            "a { transform: rotate(0.25turn); }",
            "a { transform: rotate(1.5708rad); }",
            "a { grid-template-columns: repeat(12, 1fr); }",
            "a { width: 1e4px }",
            "a { width: 1E4px }",
            "a { width: 1e10; }",
            "a { width: 8ic; }",
        ] {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    #[test]
    fn allows_units_inside_strings_comments_vars_urls() {
        for src in [
            r#"a { color: green; }"#,
            r#"a { color: green10pix; }"#,
            r#"a { width: /* 100pix */ 1em; }"#,
            r#"a::before { content: "10pix"}"#,
            r#"a { font-size: var(--some-fs-10pix); }"#,
            r#"a { margin: url(13pix); }"#,
            r#"a { margin: uRl(13pix); }"#,
            r#"a { margin: URL(13pix); }"#,
        ] {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    #[test]
    fn allows_units_in_selectors_and_property_names() {
        for src in [
            "a { margin10px: 10px; }",
            "a10pix { margin: 10px; }",
            "#a10pix { margin: 10px; }",
            ".a10pix { margin: 10px; }",
            "a:hover10pix { margin: 10px; }",
        ] {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    #[test]
    fn allows_known_media_queries() {
        for src in [
            "@media (min-width: 10px) {}",
            "@media (min-width: 10px) and (max-width: 20px) {}",
        ] {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    #[test]
    fn allows_x_in_image_set() {
        for src in [
            "a { background-image: image-set('img-1x.jpg' 1x, 'img-2x.jpg' 2x, 'img-3x.jpg' 3x) }",
            "a { background-image: -webkit-image-set('img-1x.jpg' 1x, 'img-2x.jpg' 2x) }",
            "a { background-image: url('first.png'), image-set(url('second.png') 1x) }",
            "a { background-image: image-set(url('first.png') calc(1x * 1)) }",
        ] {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    #[test]
    fn allows_x_in_resolution_media_feature() {
        for src in [
            "@media (resolution: 2x) {}",
            "@media ( resOLution: 2x) {}",
        ] {
            assert!(run(src).is_empty(), "should not fire: {src}");
        }
    }

    #[test]
    fn allows_x_in_image_resolution_property() {
        assert!(run("a { image-resolution: 1x; }").is_empty());
    }

    // --- Biome `invalid.css` fixtures: must fire. ---

    #[test]
    fn flags_unknown_units() {
        let cases = [
            ("a { font-size: 13pp; }", "pp"),
            ("a { margin: 13xpx; }", "xpx"),
            ("a { font-size: .5remm; }", "remm"),
            ("a { font-size: 0.5remm; }", "remm"),
            ("a { color: rgb(255pix, 0, 51); }", "pix"),
            ("a { color: hsl(255pix, 0, 51); }", "pix"),
            ("a { color: rgba(255pix, 0, 51, 1); }", "pix"),
            ("a { color: hsla(255pix, 0, 51, 1); }", "pix"),
            ("a { margin: calc(13pix + 10px); }", "pix"),
            ("a { margin: calc(2*10pix); }", "pix"),
            ("a { -webkit-transition-delay: 10pix; }", "pix"),
            ("a { margin: -webkit-calc(13pix + 10px); }", "pix"),
            ("a { margin: some-function(13pix + 10px); }", "pix"),
            ("root { --margin: 10pix; }", "pix"),
            ("@media (min-width: 13pix) {}", "pix"),
            ("a { width: 1e4pz; }", "pz"),
        ];
        for (src, unit) in cases {
            let diags = run(src);
            assert_eq!(diags.len(), 1, "expected one diagnostic for {src}");
            assert!(diags[0].message.contains(unit), "expected `{unit}` in {src}");
        }
    }

    #[test]
    fn unit_glued_to_multiplication_is_not_visible_to_tree_sitter() {
        // tree-sitter-css parses `pix*2` (no space before `*`) as one
        // `plain_value`, so no `unit` node exists and the rule cannot see it.
        // The spaced/operator-prefixed forms (`13pix + 10px`, `2*10pix`) keep a
        // real unit node and are flagged above.
        assert!(run("a { margin: calc(10pix*2); }").is_empty());
    }

    #[test]
    fn flags_unknown_units_case_insensitively() {
        for (src, unit) in [
            ("@media (min-width: 10px) and (max-width: 20PIX) {}", "PIX"),
            ("@media (width < 10.01REMS) {}", "REMS"),
        ] {
            let diags = run(src);
            // `width < 10.01REMS` is a range query tree-sitter parses as an
            // error, so only assert the case that produces a unit node.
            if !diags.is_empty() {
                assert!(diags[0].message.contains(unit), "expected `{unit}` in {src}");
            }
        }
        let diags = run("@media (min-width: 10px) and (max-width: 20PIX) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_x_outside_allowed_contexts() {
        for src in [
            "a { width: 400x; }",
            "@media (resolution: 2x) and (min-width: 200x) {}",
        ] {
            assert!(!run(src).is_empty(), "expected diagnostic for {src}");
        }
    }

    #[test]
    fn flags_only_the_disallowed_x_in_mixed_query() {
        // `2x` is allowed (resolution feature), `200x` is not.
        let diags = run("@media (resolution: 2x) and (min-width: 200x) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_x_in_image_set_only_for_non_x_unit() {
        // The `1pix` is unknown; the `2x` is allowed inside image-set().
        let diags = run("a { background-image: image-set('img1x.png' 1pix, 'img2x.png' 2x); }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("pix"));
    }

    #[test]
    fn flags_trailing_x_after_image_set() {
        // The `2x` units inside image-set() are valid, but the trailing `20x`
        // (outside the function) is not.
        let diags =
            run("a { background: image-set('img1x.png' 1x, 'img2x.png' 2x) left 20x / 15% 60% repeat-x; }");
        assert_eq!(diags.len(), 1);
    }
}
