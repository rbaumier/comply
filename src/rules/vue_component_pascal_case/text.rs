//! vue-component-pascal-case — Vue text backend.
//!
//! Component names in Vue templates should be PascalCase.

use rustc_hash::FxHashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

/// Known HTML and SVG element names — only these are allowed in lowercase.
const HTML_SVG_TAGS: &[&str] = &[
    // HTML
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "menu",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "search",
    "section",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
    // SVG
    "svg",
    "g",
    "path",
    "circle",
    "rect",
    "line",
    "polygon",
    "polyline",
    "text",
    "defs",
    "use",
    "mask",
    "filter",
    "stop",
    "symbol",
    "image",
    "pattern",
    "animate",
    "tspan",
    "marker",
    // SVG camelCase handled separately below
];

/// Vue framework built-in components, which are intentionally used in
/// lowercase or kebab-case (the Vue Style Guide allows it). Includes
/// vue-router's `router-view`/`router-link`. Matched case-insensitively so
/// both `<transition>` and `<Transition>` spellings are exempt.
const VUE_BUILTIN_COMPONENTS: &[&str] = &[
    "transition",
    "transition-group",
    "keep-alive",
    "teleport",
    "suspense",
    "component",
    "slot",
    "template",
    "router-view",
    "router-link",
];

/// Custom-renderer built-in components that are intentionally lowercase
/// framework escape hatches, not user-defined components. `primitive` is the
/// TresJS built-in that mounts a raw Three.js `Object3D` into the scene graph
/// (analogous to React Three Fiber's `<primitive>`); its name is fixed by the
/// renderer API, so requiring PascalCase would break it. Matched
/// case-sensitively because these names are reserved only in their lowercase
/// spelling.
const RENDERER_BUILTIN_COMPONENTS: &[&str] = &["primitive"];

/// Returns `true` for Vue framework built-in components (case-insensitive).
fn is_vue_builtin(tag: &str) -> bool {
    VUE_BUILTIN_COMPONENTS
        .iter()
        .any(|builtin| builtin.eq_ignore_ascii_case(tag))
}

/// Returns `true` for custom-renderer built-in components (case-sensitive).
fn is_renderer_builtin(tag: &str) -> bool {
    RENDERER_BUILTIN_COMPONENTS.contains(&tag)
}

/// Returns `true` for HTML/SVG built-in elements and hyphenated web components.
fn is_html_builtin(tag: &str) -> bool {
    // Hyphenated names are web components — always allowed.
    if tag.contains('-') {
        return true;
    }
    // SVG elements that use camelCase (matched case-sensitively), including
    // the SVG filter primitive elements (`fe*`).
    matches!(
        tag,
        "clipPath"
            | "linearGradient"
            | "radialGradient"
            | "animateTransform"
            | "animateMotion"
            | "foreignObject"
            | "textPath"
            | "glyphRef"
            | "feBlend"
            | "feColorMatrix"
            | "feComponentTransfer"
            | "feComposite"
            | "feConvolveMatrix"
            | "feDiffuseLighting"
            | "feDisplacementMap"
            | "feDistantLight"
            | "feDropShadow"
            | "feFlood"
            | "feFuncA"
            | "feFuncB"
            | "feFuncG"
            | "feFuncR"
            | "feGaussianBlur"
            | "feImage"
            | "feMerge"
            | "feMergeNode"
            | "feMorphology"
            | "feOffset"
            | "fePointLight"
            | "feSpecularLighting"
            | "feSpotLight"
            | "feTile"
            | "feTurbulence"
    ) || HTML_SVG_TAGS.contains(&tag)
}

/// Check if a tag name is PascalCase (starts with uppercase letter).
fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Convert a kebab-case or lowercase template tag to its PascalCase form.
/// `disabled` → `Disabled`, `basic-usage` → `BasicUsage`. Vue's template
/// compiler treats both spellings as the same component, so this is the name
/// to look up against the script's in-scope component identifiers.
fn tag_to_pascal_case(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    let mut capitalize_next = true;
    for c in tag.chars() {
        if c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            out.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Collect the PascalCase component names registered in the SFC's `<script>`
/// blocks. These are the values a template can use as components: value
/// imports (`import Disabled from './Disabled.vue'`, `import { Foo } from
/// './x'`) and Options-API `components: { Bar }` registrations. A kebab-case
/// template tag that maps to one of these is the documented kebab-case
/// spelling of a PascalCase-registered component and must not be flagged.
/// Type-only imports (`import type { User }`) are excluded: a type is never a
/// component, so a lowercase tag colliding with a type name still fires.
fn script_pascal_case_identifiers(source: &str) -> FxHashSet<String> {
    let mut tree_sitter_parser = tree_sitter::Parser::new();
    let mut idents = FxHashSet::default();
    if tree_sitter_parser
        .set_language(&tree_sitter_vue_updated::language())
        .is_err()
    {
        return idents;
    }
    let Some(tree) = tree_sitter_parser.parse(source, None) else {
        return idents;
    };
    for block in crate::rules::vue_sfc::extract_scripts(&tree, source) {
        collect_import_bindings(block.text, &mut idents);
        collect_component_registrations(block.text, &mut idents);
    }
    idents
}

/// Add the PascalCase value-import bindings from each `import ... from '...'`
/// statement. Default, named, and namespace bindings all count; `import type`
/// statements are skipped because their bindings are types, not components.
fn collect_import_bindings(text: &str, idents: &mut FxHashSet<String>) {
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(after_import) = trimmed.strip_prefix("import ") else {
            continue;
        };
        let after_import = after_import.trim_start();
        // `import type { ... }` / `import type X` bind types, never components.
        if after_import.starts_with("type ") || after_import.starts_with("type{") {
            continue;
        }
        // The clause is everything before the `from` keyword (side-effect
        // imports like `import './x'` have no clause and contribute nothing).
        let clause = after_import.split(" from ").next().unwrap_or(after_import);
        for ident in scan_identifiers(clause) {
            if ident != "type" && is_pascal_case(ident) {
                idents.insert(ident.to_string());
            }
        }
    }
}

/// Add PascalCase keys from an Options-API `components: { Foo, Bar }` block.
/// Only the identifiers inside the braces immediately following `components:`
/// are collected, so unrelated PascalCase names elsewhere in script are not.
fn collect_component_registrations(text: &str, idents: &mut FxHashSet<String>) {
    let mut rest = text;
    while let Some(pos) = rest.find("components") {
        let after = rest[pos + "components".len()..].trim_start();
        let Some(after_colon) = after.strip_prefix(':') else {
            rest = &rest[pos + "components".len()..];
            continue;
        };
        let after_colon = after_colon.trim_start();
        let Some(body) = after_colon.strip_prefix('{') else {
            rest = &rest[pos + "components".len()..];
            continue;
        };
        let end = body.find('}').unwrap_or(body.len());
        for ident in scan_identifiers(&body[..end]) {
            if is_pascal_case(ident) {
                idents.insert(ident.to_string());
            }
        }
        rest = &body[end..];
    }
}

/// Extract JS/TS identifier tokens (`[A-Za-z_$][A-Za-z0-9_$]*`) from a slice
/// of script text. Used on import clauses and registration bodies, not on
/// whole scripts, so only names in those constructs are returned.
fn scan_identifiers(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = Vec::new();
    let mut i = 0;
    let is_start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$';
    let is_part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    while i < len {
        if is_start(bytes[i]) {
            let start = i;
            i += 1;
            while i < len && is_part(bytes[i]) {
                i += 1;
            }
            out.push(&text[start..i]);
        } else {
            i += 1;
        }
    }
    out
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let script_components = script_pascal_case_identifiers(ctx.source);
        for elem in extract_elements(ctx.source) {
            // Skip HTML built-in tags, web components, Vue built-ins, and
            // custom-renderer built-ins (e.g. TresJS `<primitive>`).
            if is_html_builtin(elem.tag) || is_vue_builtin(elem.tag) || is_renderer_builtin(elem.tag)
            {
                continue;
            }
            // Non-HTML, non-PascalCase component name.
            if !is_pascal_case(elem.tag) {
                // Vue resolves a kebab-case/lowercase tag to its PascalCase
                // equivalent. When that PascalCase name is a component in
                // script scope (imported or registered), the spelling is the
                // documented kebab-case form, not a naming violation.
                if script_components.contains(&tag_to_pascal_case(elem.tag)) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-component-pascal-case".into(),
                    message: format!("Component `<{}>` should use PascalCase.", elem.tag),
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
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn allows_pascal_case() {
        let src = "<template>\n  <MyComponent />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_html_builtin() {
        let src = "<template>\n  <div></div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_custom_component() {
        let src = "<template>\n  <mycomponent />\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("mycomponent"));
    }

    #[test]
    fn allows_web_component_with_hyphen() {
        let src = "<template>\n  <my-component />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_vue_builtin_components() {
        // Issue #1490: Vue built-in components are intentionally lowercase.
        let src = "<template>\n  <transition name=\"page\" mode=\"out-in\">\n    <router-view></router-view>\n  </transition>\n  <keep-alive>\n    <component :is=\"view\" />\n  </keep-alive>\n  <teleport to=\"body\"><suspense></suspense></teleport>\n  <transition-group><router-link to=\"/\" /></transition-group>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pascal_case_vue_builtin() {
        let src = "<template>\n  <Transition><KeepAlive /></Transition>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_custom_component_alongside_builtins() {
        // Negative-space guard: a genuine lowercase custom component must still fire.
        let src = "<template>\n  <transition>\n    <mycomponent />\n  </transition>\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("mycomponent"));
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<myComponent />"));
        assert!(d.is_empty());
    }

    #[test]
    fn allows_svg_filter_primitives() {
        // Issue #4477: SVG filter primitive elements are standard SVG 2.0
        // elements (camelCase per spec), not Vue components.
        let src = "<template>\n  <svg><filter id=\"f\">\n    <feFlood flood-opacity=\"0\" />\n    <feColorMatrix type=\"matrix\" values=\"0 0 0 0 0\" />\n    <feGaussianBlur stdDeviation=\"2.7\" />\n    <feBlend mode=\"normal\" />\n  </filter></svg>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_kebab_alias_of_pascal_case_import() {
        // Issue #4709: a lowercase template tag whose PascalCase form is
        // imported in `<script setup>` is the documented kebab-case spelling
        // of that component, not a naming violation. (`<basic-usage>` is
        // already exempt via the hyphenated web-component branch, so this
        // test drives the new path through the single-word lowercase tags.)
        let src = "<script setup lang=\"ts\">\nimport Disabled from './Disabled.vue';\nimport Required from './Required.vue';\nimport Autosize from './Autosize.vue';\n</script>\n\n<template>\n  <disabled />\n  <required />\n  <autosize />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_kebab_alias_of_named_import() {
        // A named value import also registers a component in template scope.
        let src = "<script setup lang=\"ts\">\nimport { MyWidget } from './widgets';\n</script>\n\n<template>\n  <my-widget />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_kebab_alias_of_options_api_registration() {
        // Options-API `components: { Foo }` registration also exempts the
        // kebab-case spelling of the registered component.
        let src = "<script>\nexport default {\n  components: { FooBar },\n};\n</script>\n\n<template>\n  <foo-bar />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_component_without_matching_import() {
        // Negative-space guard: a lowercase tag with no corresponding
        // PascalCase import or registration must still fire.
        let src = "<script setup lang=\"ts\">\nimport Disabled from './Disabled.vue';\n</script>\n\n<template>\n  <disabled />\n  <required />\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("required"));
    }

    #[test]
    fn flags_lowercase_tag_matching_type_only_import() {
        // Boundary: a type-only import is not a component, so a lowercase tag
        // colliding with the type name is still a genuine violation.
        let src = "<script setup lang=\"ts\">\nimport type { User } from './types';\n</script>\n\n<template>\n  <user />\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("user"));
    }

    #[test]
    fn allows_svg_text_path() {
        // Issue #4762: `<textPath>` is a standard SVG element (camelCase per
        // the SVG 1.1 spec), used inside `<text>` to render along a path; it is
        // not a Vue component.
        let src = "<template>\n  <text>\n    <textPath :href=\"`#${id}`\" startOffset=\"50%\" text-anchor=\"middle\">{{ data.text }}</textPath>\n  </text>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_svg_camel_case_animation_elements() {
        // The remaining standard camelCase SVG element names are native SVG
        // elements, not Vue components.
        let src = "<template>\n  <svg>\n    <animateMotion dur=\"2s\" />\n    <glyphRef />\n  </svg>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tresjs_primitive_builtin() {
        // Issue #4829: TresJS `<primitive>` is a built-in renderer escape hatch
        // (mounts a raw Three.js Object3D); its lowercase name is fixed by the
        // API and must not be flagged.
        let src = "<template>\n  <primitive :object=\"pool.shadowGroup\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_custom_component_alongside_primitive() {
        // Negative-space guard: a genuine lowercase custom component must still
        // fire even when a renderer built-in is present.
        let src = "<template>\n  <primitive :object=\"obj\" />\n  <myWidget />\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("myWidget"));
    }

    #[test]
    fn flags_camel_case_non_svg_component() {
        // Negative-space guard: a genuine camelCase custom component (not an
        // SVG element) must still fire.
        let src = "<template>\n  <myComponent />\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("myComponent"));
    }
}
