//! use-vue-hyphenated-attributes AST backend.
//!
//! Walks `attribute` and `directive_attribute` nodes. For plain HTML attributes
//! it checks the `attribute_name`; for `:foo` shorthand binds and `v-bind:`/
//! `v-model:` directives it checks the static `directive_argument`. An attribute
//! whose name is neither kebab-case nor pure-lowercase is reported. Attributes on
//! SVG-exclusive elements (`<svg>`, `<path>`, ...) are skipped, mirroring Biome's
//! `useVueHyphenatedAttributes`. Bindings on a `<slot>` element are also skipped:
//! they are slot props, not DOM attributes or component props.

use crate::diagnostic::{Diagnostic, Severity};

/// SVG-exclusive element tag names. Attributes on these elements legitimately use
/// camelCase (`viewBox`, `gradientUnits`, ...) so the rule skips them entirely.
/// Mirrors Biome's `SVG_EXCLUSIVE_ELEMENTS` (kept sorted for `binary_search`).
const SVG_EXCLUSIVE_ELEMENTS: &[&str] = &[
    "altGlyph",
    "altGlyphDef",
    "altGlyphItem",
    "animate",
    "animateColor",
    "animateMotion",
    "animateTransform",
    "circle",
    "clipPath",
    "color-profile",
    "cursor",
    "defs",
    "desc",
    "discard",
    "ellipse",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "filter",
    "font",
    "font-face",
    "font-face-format",
    "font-face-name",
    "font-face-src",
    "font-face-uri",
    "foreignObject",
    "g",
    "glyph",
    "glyphRef",
    "hatch",
    "hatchpath",
    "hkern",
    "image",
    "line",
    "linearGradient",
    "marker",
    "mask",
    "mesh",
    "meshgradient",
    "meshpatch",
    "meshrow",
    "metadata",
    "missing-glyph",
    "mpath",
    "path",
    "pattern",
    "polygon",
    "polyline",
    "radialGradient",
    "rect",
    "set",
    "solidcolor",
    "stop",
    "svg",
    "switch",
    "symbol",
    "text",
    "textPath",
    "tref",
    "tspan",
    "use",
    "view",
    "vkern",
];

/// Whether the attribute name is hyphenated (kebab-case) or pure lowercase, the
/// two forms Biome accepts (`Case::Kebab | Case::Lower`).
///
/// Accepts names whose first character is a lowercase letter or digit and whose
/// remaining characters are lowercase letters, digits, or `-`, with no leading,
/// trailing, or consecutive `-`. Any uppercase letter, `_`, or caseless
/// alphanumeric (e.g. CJK) makes the name non-hyphenated and therefore a
/// violation.
fn is_hyphenated(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    let mut previous = first;
    for current in chars {
        match current {
            '-' if previous != '-' => {}
            c if c.is_lowercase() || c.is_ascii_digit() => {}
            _ => return false,
        }
        previous = current;
    }
    previous != '-'
}

/// The tag name of the `start_tag`/`self_closing_tag` that owns this attribute.
fn owning_tag_name<'a>(attribute: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let tag = attribute.parent()?;
    let mut cursor = tag.walk();
    tag.children(&mut cursor)
        .find(|c| c.kind() == "tag_name")
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether the owning element is SVG-exclusive (attributes on it are skipped).
fn is_on_svg_element(attribute: tree_sitter::Node, source: &[u8]) -> bool {
    owning_tag_name(attribute, source)
        .is_some_and(|tag| SVG_EXCLUSIVE_ELEMENTS.binary_search(&tag).is_ok())
}

/// A `:`/`v-bind`/`v-model` binding on a `<slot>` element is a slot prop, not a
/// DOM attribute or component prop. Slot props are consumed by exact name via JS
/// destructuring (`v-slot="{ isOpen }"`) with no kebab↔camel normalization, so the
/// hyphenation convention does not apply.
fn is_slot_prop_binding(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "directive_attribute" && owning_tag_name(node, source) == Some("slot")
}

/// Read the text of the first child of `node` with the given kind.
fn child_text<'a>(node: tree_sitter::Node, kind: &str, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == kind)
        .and_then(|n| n.utf8_text(source).ok())
}

/// The attribute name to validate, or `None` when the node is out of scope.
///
/// - plain `attribute` → its `attribute_name`.
/// - `directive_attribute` with name `:`, `v-bind`, or `v-model` → its static
///   `directive_argument`. Dynamic arguments (`:[key]`), argument-less directives,
///   and any other directive (`v-on`, `@`, `v-if`, ...) are skipped.
fn checked_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "attribute" => child_text(node, "attribute_name", source),
        "directive_attribute" => {
            let directive_name = child_text(node, "directive_name", source)?;
            if !matches!(directive_name, ":" | "v-bind" | "v-model") {
                return None;
            }
            child_text(node, "directive_argument", source)
        }
        _ => None,
    }
}

crate::ast_check! { on ["attribute", "directive_attribute"] => |node, source, ctx, diagnostics|
    let Some(name) = checked_name(node, source) else {
        return;
    };
    if is_hyphenated(name) || is_on_svg_element(node, source) || is_slot_prop_binding(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!("Attribute `{name}` should be hyphenated (kebab-case)."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    fn wrap(body: &str) -> String {
        format!("<template>\n{body}\n</template>")
    }

    // --- Invalid fixtures (Biome invalid.vue) ---

    #[test]
    fn flags_camelcase_plain_attribute() {
        let diags = run(&wrap("<div fooBar=\"x\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fooBar"));
        assert!(diags[0].message.contains("hyphenated"));
    }

    #[test]
    fn flags_camelcase_shorthand_bind_prop() {
        let diags = run(&wrap("<MyComp :someProp=\"x\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("someProp"));
    }

    #[test]
    fn flags_camelcase_v_bind_longhand_argument() {
        assert_eq!(run(&wrap("<div v-bind:fooBar=\"x\" />")).len(), 1);
    }

    #[test]
    fn flags_camelcase_v_model_argument() {
        assert_eq!(run(&wrap("<input v-model:fooBar=\"x\" />")).len(), 1);
    }

    #[test]
    fn flags_pascalcase_attribute() {
        assert_eq!(run(&wrap("<div FooBar=\"x\" />")).len(), 1);
    }

    #[test]
    fn flags_snake_case_attribute() {
        assert_eq!(run(&wrap("<div foo_bar=\"x\" />")).len(), 1);
    }

    #[test]
    fn flags_constant_case_attribute() {
        assert_eq!(run(&wrap("<div FOO_BAR=\"x\" />")).len(), 1);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_kebab_case_plain_attribute() {
        assert!(run(&wrap("<div data-test-id=\"x\"></div>")).is_empty());
    }

    #[test]
    fn allows_pure_lowercase_attribute() {
        assert!(run(&wrap("<div class=\"foo\"></div>")).is_empty());
    }

    #[test]
    fn allows_kebab_case_shorthand_bind() {
        assert!(run(&wrap("<MyComp :some-prop=\"x\" />")).is_empty());
    }

    #[test]
    fn allows_aria_and_data_attributes() {
        assert!(run(&wrap("<div aria-label=\"x\" data-id=\"y\" />")).is_empty());
    }

    #[test]
    fn allows_kebab_with_digits() {
        assert!(run(&wrap("<div data-id-2=\"x\" />")).is_empty());
    }

    // --- SVG exemption (root cause: element-level, not attribute allowlist) ---

    #[test]
    fn allows_camelcase_attr_on_svg_element() {
        assert!(run(&wrap("<svg viewBox=\"0 0 1 1\" />")).is_empty());
    }

    #[test]
    fn allows_camelcase_attr_on_other_svg_exclusive_element() {
        assert!(run(&wrap("<linearGradient gradientUnits=\"userSpaceOnUse\" />")).is_empty());
    }

    #[test]
    fn flags_camelcase_attr_on_non_svg_element() {
        // `viewBox` outside an SVG-exclusive element is still a violation.
        assert_eq!(run(&wrap("<div viewBox=\"x\" />")).len(), 1);
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_dynamic_argument_bind() {
        // `:[key]` has a dynamic argument, not a static prop name → skipped.
        assert!(run(&wrap("<div :[fooBar]=\"x\" />")).is_empty());
    }

    #[test]
    fn ignores_v_on_directive_argument() {
        // Event handlers (`v-on:`/`@`) are not checked by this rule.
        assert!(run(&wrap("<div v-on:fooBar=\"x\" />")).is_empty());
        assert!(run(&wrap("<div @fooBar=\"x\" />")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" v-show=\"yes\" v-for=\"x in xs\" />")).is_empty());
    }

    #[test]
    fn ignores_argumentless_v_bind() {
        assert!(run(&wrap("<MyComp v-bind=\"props\" />")).is_empty());
    }

    #[test]
    fn ignores_lowercase_event_like_attribute() {
        assert!(run(&wrap("<button onclick=\"go()\">x</button>")).is_empty());
    }

    // --- Slot-prop exemption (element-level: bindings on `<slot>` are slot props) ---

    #[test]
    fn allows_camelcase_shorthand_bind_on_slot() {
        // `:isOpen` on `<slot>` is a slot prop consumed by exact name → not flagged.
        assert!(run(&wrap("<slot name=\"trigger\" :isOpen=\"open\" />")).is_empty());
    }

    #[test]
    fn allows_multiple_camelcase_slot_prop_bindings() {
        assert!(
            run(&wrap(
                "<slot name=\"trigger\" :toggle=\"onMenuToggle\" :isOpen=\"isOpen\" />"
            ))
            .is_empty()
        );
    }

    #[test]
    fn allows_v_bind_longhand_on_slot() {
        assert!(run(&wrap("<slot v-bind:fooBar=\"x\" />")).is_empty());
    }

    #[test]
    fn flags_camelcase_shorthand_bind_on_component() {
        // The slot exemption is `<slot>`-only: the same binding on a component flags.
        let diags = run(&wrap("<MyComp :isOpen=\"open\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("isOpen"));
    }

    #[test]
    fn flags_plain_camelcase_attribute_on_slot() {
        // Only directive bindings are exempt on `<slot>`; a plain attr still flags.
        assert_eq!(run(&wrap("<slot fooBar=\"x\" />")).len(), 1);
    }
}
