//! use-vue-hyphenated-attributes AST backend.
//!
//! Walks `attribute` and `directive_attribute` nodes. For plain HTML attributes
//! it checks the `attribute_name`; for `:foo` shorthand binds and `v-bind:`/
//! `v-model:` directives it checks the static `directive_argument`. An attribute
//! whose name is neither kebab-case nor pure-lowercase is reported. Canonical
//! camelCase HTML/DOM attributes (`frameBorder`, `allowFullScreen`, `tabIndex`,
//! ...) are skipped: their spec form is camelCase, not a custom Vue prop.
//! Attributes on SVG-exclusive elements (`<svg>`, `<path>`, ...) are skipped,
//! mirroring Biome's `useVueHyphenatedAttributes`. Attributes on TresJS elements (`Tres`-prefixed
//! components and `<primitive>`) are skipped: their names carry camelCase Three.js
//! property segments that must stay camelCase. Bindings on a `<slot>` element are
//! also skipped: they are slot props, not DOM attributes or component props.

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

/// Canonical camelCase HTML/DOM attribute names. These are standard attributes
/// whose spec/DOM-property form is camelCase (the React-style spelling), not
/// custom Vue props, so hyphenating them does not match any real attribute and
/// can silently break the element. They are accepted on any element (kept sorted
/// for `binary_search`). The SVG camelCase attributes are handled separately via
/// `SVG_EXCLUSIVE_ELEMENTS`.
const CAMEL_CASE_HTML_ATTRIBUTES: &[&str] = &[
    "acceptCharset",
    "accessKey",
    "allowFullScreen",
    "autoCapitalize",
    "autoComplete",
    "autoCorrect",
    "autoFocus",
    "autoPlay",
    "autoSave",
    "cellPadding",
    "cellSpacing",
    "charSet",
    "classID",
    "colSpan",
    "contentEditable",
    "contextMenu",
    "controlsList",
    "crossOrigin",
    "dateTime",
    "disablePictureInPicture",
    "disableRemotePlayback",
    "encType",
    "enterKeyHint",
    "fetchPriority",
    "formAction",
    "formEncType",
    "formMethod",
    "formNoValidate",
    "formTarget",
    "frameBorder",
    "hrefLang",
    "httpEquiv",
    "imageSizes",
    "imageSrcSet",
    "inputMode",
    "itemID",
    "itemProp",
    "itemRef",
    "itemScope",
    "itemType",
    "marginHeight",
    "marginWidth",
    "maxLength",
    "mediaGroup",
    "minLength",
    "noModule",
    "noValidate",
    "radioGroup",
    "readOnly",
    "referrerPolicy",
    "rowSpan",
    "tabIndex",
    "useMap",
];

/// Whether the attribute name is a canonical camelCase HTML/DOM attribute
/// (`frameBorder`, `allowFullScreen`, `tabIndex`, ...), which is a standard
/// attribute spelled in camelCase rather than a custom Vue prop.
fn is_camel_case_html_attribute(name: &str) -> bool {
    CAMEL_CASE_HTML_ATTRIBUTES.binary_search(&name).is_ok()
}

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

/// Whether the owning element is a TresJS component (attributes on it are skipped).
///
/// TresJS (a Three.js renderer for Vue) exposes Three.js object properties as
/// component props, and many Three.js properties are inherently camelCase
/// (`castShadow`, `toneMapping`, ...). Hyphens in TresJS attribute names are
/// property-path delimiters (`uniforms-fresnelAmount-value` → `material.uniforms.
/// fresnelAmount.value`), so the camelCase path segments must stay camelCase —
/// hyphenating them breaks the prop→Three.js mapping. TresJS components are the
/// `Tres`-prefixed family (`TresMesh`, `TresCanvas`, `TresHolographicMaterial`,
/// ...) plus the `<primitive>` catch-all element that mounts arbitrary Three.js
/// objects.
fn is_on_tresjs_element(attribute: tree_sitter::Node, source: &[u8]) -> bool {
    owning_tag_name(attribute, source).is_some_and(|tag| {
        tag == "primitive"
            || tag
                .strip_prefix("Tres")
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
    })
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
/// - plain `attribute` → its `attribute_name`, unless the name contains `:`, `--`,
///   or `.`. None of these can occur in a JavaScript identifier, so a name bearing
///   one is a token from another sublanguage rather than a component prop or DOM
///   attribute subject to the kebab-case convention: `:` is a namespaced /
///   variant-prefixed name (XML/SVG namespace like `xlink:href`, or a UnoCSS/Windi
///   attributify variant like `md:grid-cols-2`); `--` is UnoCSS attributify
///   negative-value notation (`me--4`, `z--1`); `.` is a UnoCSS attributify decimal
///   value (`gap-0.5`, `px1.2`) or the trailing member-expression segment of a
///   `<motion.div>` element tag.
/// - `directive_attribute` with name `:`, `v-bind`, or `v-model` → its static
///   `directive_argument`. Dynamic arguments (`:[key]`), argument-less directives,
///   and any other directive (`v-on`, `@`, `v-if`, ...) are skipped.
fn checked_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "attribute" => {
            let name = child_text(node, "attribute_name", source)?;
            // `:`, `--`, and `.` cannot occur in a JavaScript identifier, so a plain
            // attribute name containing any of them is not a component prop or DOM
            // attribute subject to the kebab-case convention:
            // - `:` → namespaced / variant-prefixed name (`xlink:href`,
            //   `md:grid-cols-2`).
            // - `--` → UnoCSS attributify negative-value notation (`me--4` =
            //   `margin-inline-end: -1rem`, `z--1`, `inset-ie--10`).
            // - `.` → UnoCSS attributify decimal/fractional value (`gap-0.5`,
            //   `px1.2`, `mx-1.25rem`), or the trailing member-expression segment of
            //   a `<motion.div>` element tag (the grammar splits `motion.div` into a
            //   `motion` `tag_name` plus a spurious `.div` attribute).
            if name.contains(':') || name.contains("--") || name.contains('.') {
                return None;
            }
            Some(name)
        }
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
    if is_hyphenated(name)
        || is_camel_case_html_attribute(name)
        || is_on_svg_element(node, source)
        || is_on_tresjs_element(node, source)
        || is_slot_prop_binding(node, source)
    {
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

    #[test]
    fn allows_svg_camelcase_bindings_on_marker_multiline() {
        // Issue #4850: vue-flow's `<marker>` with `:markerWidth`/`:markerHeight`/
        // `:markerUnits` shorthand bindings (SVG case-sensitive attributes).
        let diags = run(&wrap(
            "<marker\n  refY=\"0\"\n  :markerWidth=\"`${width}`\"\n  :markerHeight=\"`${height}`\"\n  :markerUnits=\"markerUnits\"\n  :orient=\"orient\"\n>\n</marker>",
        ));
        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    }

    #[test]
    fn flags_camelcase_bindings_on_non_svg_element_multiline() {
        // Negative control for the multiline parse: the same shorthand bindings on
        // a non-SVG-exclusive element are genuine violations and must still flag,
        // proving the empty result above comes from the SVG exemption, not a
        // degenerate multiline parse.
        let diags = run(&wrap(
            "<div\n  :markerWidth=\"`${width}`\"\n  :markerHeight=\"`${height}`\"\n  :markerUnits=\"markerUnits\"\n>\n</div>",
        ));
        assert_eq!(diags.len(), 3, "unexpected diags: {diags:?}");
    }

    // --- Canonical camelCase HTML/DOM attributes (name-level allowlist) ---

    #[test]
    fn allows_camelcase_html_attributes_on_iframe() {
        // Issue #5112: `frameBorder` and `allowFullScreen` are spec camelCase HTML
        // attributes on `<iframe>`, not custom Vue props → not flagged.
        let diags = run(&wrap("<iframe frameBorder=\"0\" allowFullScreen />"));
        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    }

    #[test]
    fn allows_camelcase_html_attribute_binding() {
        // The allowlist is name-based, so the same attribute as a `:` binding is
        // also accepted.
        assert!(run(&wrap("<iframe :frameBorder=\"border\" />")).is_empty());
        assert!(run(&wrap("<input :tabIndex=\"idx\" :readOnly=\"ro\" />")).is_empty());
    }

    #[test]
    fn allows_contenteditable_and_other_camelcase_html_attributes() {
        assert!(run(&wrap("<div contentEditable=\"true\" tabIndex=\"0\" />")).is_empty());
    }

    #[test]
    fn flags_custom_camelcase_prop_not_in_html_allowlist() {
        // The allowlist only covers canonical HTML attributes; a genuine custom
        // camelCase prop is still flagged and should be hyphenated.
        let diags = run(&wrap("<div myCustomProp=\"x\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("myCustomProp"));
    }

    // --- TresJS exemption (element-level: `Tres`-prefixed components + `<primitive>`) ---

    #[test]
    fn allows_camelcase_property_path_bindings_on_tresjs_component() {
        // Issue #4830: TresJS property-path attributes use hyphens as delimiters and
        // carry camelCase Three.js property segments (`fresnelAmount`, `mapSize`)
        // that must stay camelCase.
        let diags = run(&wrap(
            "<TresHolographicMaterial\n  :uniforms-fresnelAmount-value=\"props.fresnelAmount\"\n  :shadow-mapSize-width=\"1024\"\n  :shadow-mapSize-height=\"1024\"\n/>",
        ));
        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    }

    #[test]
    fn allows_camelcase_attr_on_tresjs_component() {
        // Plain camelCase Three.js props on a `Tres`-prefixed component.
        assert!(run(&wrap("<TresMesh castShadow receiveShadow />")).is_empty());
    }

    #[test]
    fn allows_camelcase_attr_on_primitive_element() {
        // `<primitive>` mounts arbitrary Three.js objects via camelCase props.
        assert!(run(&wrap("<primitive :rotation-order=\"`XYZ`\" :castShadow=\"true\" />")).is_empty());
    }

    #[test]
    fn flags_camelcase_attr_on_non_tres_component() {
        // The exemption is `Tres`-prefixed-only: `Tres` must be followed by an
        // uppercase letter, so a user component like `<Trespasser>` is not TresJS
        // and a genuine camelCase prop on it still flags.
        let diags = run(&wrap("<Trespasser :fooBar=\"x\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fooBar"));
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

    // --- Colon-bearing plain attributes (UnoCSS/Windi attributify + XML/SVG namespaces) ---

    #[test]
    fn allows_unocss_responsive_attributify_utility() {
        // Issue #4383: `md:grid-cols-2` is a UnoCSS attributify utility, the `:` is the
        // `md:` breakpoint variant, not a Vue namespace → not flagged.
        assert!(run(&wrap("<div grid md:grid-cols-2 gap-2></div>")).is_empty());
    }

    #[test]
    fn allows_unocss_state_and_theme_variants() {
        assert!(run(&wrap("<div hover:bg-red dark:text-white 2xl:gap-4 />")).is_empty());
    }

    #[test]
    fn allows_xml_namespace_plain_attribute() {
        // `<a>` is not SVG-exclusive, so this locks the colon guard, not the SVG exemption.
        assert!(run(&wrap("<a xlink:href=\"#x\" />")).is_empty());
    }

    #[test]
    fn flags_camelcase_plain_attribute_without_colon() {
        // The colon guard only skips colon-bearing names; a plain camelCase attribute
        // with no colon is still a genuine kebab-case violation.
        assert_eq!(run(&wrap("<div myProp=\"x\" />")).len(), 1);
    }

    #[test]
    fn flags_camelcase_directive_argument_with_colon() {
        // `:fooBar` flows through the directive arm (argument validation), which the
        // plain-attribute colon guard does not touch → still flagged.
        assert_eq!(run(&wrap("<div :fooBar=\"x\" />")).len(), 1);
    }

    // --- UnoCSS attributify value notation (negative `--`, decimal `.`) ---

    #[test]
    fn allows_unocss_attributify_negative_value() {
        // Issue #6191: in UnoCSS attributify mode a negative value is written with a
        // double hyphen (`me--4` = `margin-inline-end: -1rem`). `--` cannot occur in
        // a JavaScript identifier, so these are utility class names, not props.
        assert!(run(&wrap("<div me--4 z--1 inset-ie--10 />")).is_empty());
    }

    #[test]
    fn allows_unocss_attributify_decimal_value() {
        // Issue #6191: fractional UnoCSS values use a dot (`gap-0.5`, `px1.2`,
        // `py0.2`, `mx-1.25rem`). A `.` cannot occur in a JavaScript identifier.
        assert!(run(&wrap("<div gap-0.5 px1.2 py0.2 mx-1.25rem />")).is_empty());
    }

    #[test]
    fn flags_camelcase_prop_without_unocss_value_notation() {
        // Negative space: a genuine camelCase prop carries neither `--` nor `.`, so
        // the UnoCSS value-notation guards do not exempt it.
        let diags = run(&wrap("<div maxRetries=\"3\" />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetries"));
    }

    // --- Member-expression element tags (`<Foo.bar>`, e.g. motion-v) ---

    #[test]
    fn allows_motion_v_dot_element_attributes() {
        // Issue #4672: `<motion.div>` is the motion-v component identifier
        // `motion.div`; the grammar splits it into a `motion` tag with a spurious
        // `.div` attribute. The `.div` segment is part of the component name, not
        // an attribute, so it must not be flagged.
        assert!(
            run(&wrap(
                "<motion.div layout class=\"rounded-lg\">\
                 <motion.legend id=\"feedback-legend\">Was this helpful?</motion.legend>\
                 </motion.div>"
            ))
            .is_empty()
        );
        // Self-closing tags and deeper member chains (`<motion.svg.path>`) fold the
        // whole tail into one `.`-prefixed attribute name → also skipped.
        assert!(run(&wrap("<motion.div layout />")).is_empty());
        assert!(run(&wrap("<motion.svg.path d=\"x\" />")).is_empty());
    }

    #[test]
    fn flags_real_camelcase_attribute_on_dot_element() {
        // The guard only skips the `.bar` member-segment artifact; a genuine
        // camelCase attribute on the same element is still flagged.
        assert_eq!(run(&wrap("<motion.div fooBar=\"x\"></motion.div>")).len(), 1);
    }
}
