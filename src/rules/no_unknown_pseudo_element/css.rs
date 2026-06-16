use crate::diagnostic::{Diagnostic, Severity};

// Known pseudo-element names, ported verbatim from Biome's `biome_css_analyze`
// keyword tables (`@c2fd653`). Each array is sorted so membership is a
// `binary_search`. Matching is case-insensitive: callers pass an
// already-lowercased name.

/// Pseudo-elements that also accept single-colon notation
/// (`LEVEL_ONE_AND_TWO_PSEUDO_ELEMENTS`).
const LEVEL_ONE_AND_TWO_PSEUDO_ELEMENTS: &[&str] =
    &["after", "before", "first-letter", "first-line", "slotted"];

/// Vendor-specific pseudo-elements spelled out in full
/// (`VENDOR_SPECIFIC_PSEUDO_ELEMENTS`). Vendor-prefixed names not on this list
/// are still accepted by the `-webkit-`/`-moz-`/… prefix check.
const VENDOR_SPECIFIC_PSEUDO_ELEMENTS: &[&str] = &[
    "-moz-focus-inner",
    "-moz-focus-outer",
    "-moz-list-bullet",
    "-moz-meter-bar",
    "-moz-placeholder",
    "-moz-progress-bar",
    "-moz-range-progress",
    "-moz-range-thumb",
    "-moz-range-track",
    "-ms-browse",
    "-ms-check",
    "-ms-clear",
    "-ms-expand",
    "-ms-fill",
    "-ms-fill-lower",
    "-ms-fill-upper",
    "-ms-reveal",
    "-ms-thumb",
    "-ms-ticks-after",
    "-ms-ticks-before",
    "-ms-tooltip",
    "-ms-track",
    "-ms-value",
    "-webkit-calendar-picker-indicator",
    "-webkit-clear-button",
    "-webkit-color-swatch",
    "-webkit-color-swatch-wrapper",
    "-webkit-date-and-time-value",
    "-webkit-datetime-edit",
    "-webkit-datetime-edit-ampm-field",
    "-webkit-datetime-edit-day-field",
    "-webkit-datetime-edit-fields-wrapper",
    "-webkit-datetime-edit-hour-field",
    "-webkit-datetime-edit-millisecond-field",
    "-webkit-datetime-edit-minute-field",
    "-webkit-datetime-edit-month-field",
    "-webkit-datetime-edit-second-field",
    "-webkit-datetime-edit-text",
    "-webkit-datetime-edit-week-field",
    "-webkit-datetime-edit-year-field",
    "-webkit-details-marker",
    "-webkit-distributed",
    "-webkit-file-upload-button",
    "-webkit-input-placeholder",
    "-webkit-keygen-select",
    "-webkit-meter-bar",
    "-webkit-meter-even-less-good-value",
    "-webkit-meter-inner-element",
    "-webkit-meter-optimum-value",
    "-webkit-meter-suboptimum-value",
    "-webkit-progress-bar",
    "-webkit-progress-inner-element",
    "-webkit-progress-value",
    "-webkit-search-cancel-button",
    "-webkit-search-decoration",
    "-webkit-search-results-button",
    "-webkit-search-results-decoration",
    "-webkit-slider-runnable-track",
    "-webkit-slider-thumb",
    "-webkit-textfield-decoration-container",
    "-webkit-validation-bubble",
    "-webkit-validation-bubble-arrow",
    "-webkit-validation-bubble-arrow-clipper",
    "-webkit-validation-bubble-heading",
    "-webkit-validation-bubble-message",
    "-webkit-validation-bubble-text-block",
];

/// Shadow-tree pseudo-elements (`SHADOW_TREE_PSEUDO_ELEMENTS`).
const SHADOW_TREE_PSEUDO_ELEMENTS: &[&str] = &["part"];

const OTHER_PSEUDO_ELEMENTS: &[&str] = &[
    "backdrop",
    "checkmark",
    "column",
    "content",
    "cue",
    "details-content",
    "file-selector-button",
    "grammar-error",
    "highlight",
    "marker",
    "picker",
    "picker-icon",
    "placeholder",
    "prefix",
    "scroll-button",
    "scroll-marker",
    "scroll-marker-group",
    "search-text",
    "selection",
    "shadow",
    "slotted",
    "spelling-error",
    "suffix",
    "target-text",
    "view-transition",
    "view-transition-group",
    "view-transition-image-pair",
    "view-transition-new",
    "view-transition-old",
];

/// CSS Modules pseudo-elements, valid only in `.module.css` files
/// (`["global", "local"]`).
const CSS_MODULE_PSEUDO_ELEMENTS: &[&str] = &["global", "local"];

/// `is_known_pseudo_element`: the union of every standard pseudo-element table.
fn is_known_pseudo_element(name: &str) -> bool {
    LEVEL_ONE_AND_TWO_PSEUDO_ELEMENTS.binary_search(&name).is_ok()
        || VENDOR_SPECIFIC_PSEUDO_ELEMENTS.binary_search(&name).is_ok()
        || SHADOW_TREE_PSEUDO_ELEMENTS.binary_search(&name).is_ok()
        || OTHER_PSEUDO_ELEMENTS.binary_search(&name).is_ok()
}

/// `vender_prefix`: a recognised vendor prefix.
fn vendor_prefixed(name: &str) -> bool {
    name.starts_with("-webkit-")
        || name.starts_with("-moz-")
        || name.starts_with("-ms-")
        || name.starts_with("-o-")
}

/// Biome scopes CSS Modules pseudo-elements to CSS Modules files; mirror that by
/// gating on the `.module.css` extension.
fn is_css_module(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".module.css"))
}

/// The pseudo-element name of a `pseudo_element_selector` node: the `tag_name`
/// child immediately after the `::` token. For functional pseudo-elements
/// (`::part(x)`, `::picker(x)`) this is the function name; the argument lives in
/// a separate `arguments` child. A leading type selector (`a` in `a::before`)
/// is a `tag_name` *before* the `::`, so it is excluded.
fn pseudo_element_name<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    let sep_start = node
        .children(&mut cursor)
        .find(|c| c.kind() == "::")
        .map(|c| c.start_byte())?;
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "tag_name" && c.start_byte() > sep_start)
        .and_then(|c| c.utf8_text(source).ok())
}

crate::ast_check! { on ["pseudo_element_selector"] => |node, source, ctx, diagnostics|
    let Some(name) = pseudo_element_name(&node, source) else {
        return;
    };
    // `::foo-#{$name}` (Sass interpolation) cannot be validated; tree-sitter
    // leaves a truncated `tag_name` ending in `-` plus a sibling ERROR node.
    // Skip names that aren't a complete identifier.
    if name.is_empty() || name.contains('#') || name.ends_with('-') {
        return;
    }
    let lower = name.to_ascii_lowercase();
    let lower = lower.as_str();

    if vendor_prefixed(lower) || is_known_pseudo_element(lower) {
        return;
    }
    if is_css_module(ctx.path) && CSS_MODULE_PSEUDO_ELEMENTS.binary_search(&lower).is_ok() {
        return;
    }
    let ignore = ctx
        .config
        .string_list("no-unknown-pseudo-element", "ignore", ctx.lang);
    if ignore.iter().any(|p| p.eq_ignore_ascii_case(name)) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Unexpected unknown pseudo-element `{name}`."),
        Severity::Warning,
    ));
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

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
    }

    // --- Biome `valid.css`: must not fire. ---

    #[test]
    fn allows_single_colon_level_one_two_elements() {
        // Single-colon `:before` etc. parse as pseudo-classes, never reaching
        // this rule, so they cannot be flagged here.
        assert!(run("a:before { }").is_empty());
        assert!(run("a:Before { }").is_empty());
        assert!(run("a:bEfOrE { }").is_empty());
        assert!(run("a:BEFORE { }").is_empty());
        assert!(run("a:after { }").is_empty());
        assert!(run("a:first-letter { }").is_empty());
        assert!(run("a:first-line { }").is_empty());
    }

    #[test]
    fn allows_double_colon_level_one_two_elements_case_insensitive() {
        assert!(run("a::before { }").is_empty());
        assert!(run("a::Before { }").is_empty());
        assert!(run("a::bEfOrE { }").is_empty());
        assert!(run("a::BEFORE { }").is_empty());
        assert!(run("a::after { }").is_empty());
        assert!(run("a::first-letter { }").is_empty());
        assert!(run("a::first-line { }").is_empty());
    }

    #[test]
    fn allows_other_known_elements() {
        assert!(run("::selection { }").is_empty());
        assert!(run("a::spelling-error { }").is_empty());
        assert!(run("a::grammar-error { }").is_empty());
        assert!(run("li::marker { }").is_empty());
        assert!(run("div::shadow { }").is_empty());
        assert!(run("div::content { }").is_empty());
    }

    #[test]
    fn allows_vendor_specific_and_vendor_prefixed_elements() {
        // On the vendor-specific list.
        assert!(run("input::-moz-placeholder { }").is_empty());
        // Not on the list, but accepted via the vendor-prefix check.
        assert!(run("input::-moz-test { }").is_empty());
    }

    #[test]
    fn allows_pseudo_class_before_pseudo_element() {
        assert!(run("a:hover::before { }").is_empty());
        assert!(run("a:hover::-moz-placeholder { }").is_empty());
    }

    #[test]
    fn allows_selector_list_with_combinator() {
        assert!(run("a,\nb > .foo::before { }").is_empty());
    }

    #[test]
    fn allows_custom_property_declarations() {
        // No pseudo-element selectors here at all.
        assert!(run(":root { --foo: 1px; }").is_empty());
        assert!(run("html { --foo: 1px; }").is_empty());
        assert!(run(":root { --custom-property-set: {} }").is_empty());
        assert!(run("html { --custom-property-set: {} }").is_empty());
    }

    #[test]
    fn allows_functional_part() {
        assert!(run("a::part(shadow-part) { }").is_empty());
    }

    #[test]
    fn allows_functional_view_transition() {
        assert!(
            run("::view-transition-old(*),\n::view-transition-new(*) {\n\tposition: absolute;\n}")
                .is_empty()
        );
    }

    #[test]
    fn allows_recently_specified_elements() {
        assert!(run("details::details-content {}").is_empty());
        assert!(run("select::picker(select) {}").is_empty());
        assert!(run("option::checkmark {}").is_empty());
        assert!(run("select::picker-icon {}").is_empty());
        assert!(run(".carousel::scroll-marker {}").is_empty());
        assert!(run(".carousel::scroll-marker-group {}").is_empty());
        assert!(run(".carousel::scroll-button(inline-start) {}").is_empty());
        assert!(run(".multicol::column {}").is_empty());
    }

    // --- Biome `invalid.css`: must fire. ---

    #[test]
    fn flags_unknown_case_insensitive() {
        assert_eq!(run("a::pseudo { }").len(), 1);
        assert_eq!(run("a::Pseudo { }").len(), 1);
        assert_eq!(run("a::pSeUdO { }").len(), 1);
        assert_eq!(run("a::PSEUDO { }").len(), 1);
    }

    #[test]
    fn flags_unknown_element() {
        assert_eq!(run("a::element { }").len(), 1);
    }

    #[test]
    fn flags_unknown_after_pseudo_class() {
        assert_eq!(run("a:hover::element { }").len(), 1);
    }

    #[test]
    fn flags_unknown_in_selector_list() {
        assert_eq!(run("a,\nb > .foo::error { }").len(), 1);
    }

    // --- Biome `valid.module.css`: CSS Modules `::global` / `::local`. ---
    // The `::after`/`::before` nesting case (`& ::after`) is also covered.

    #[test]
    fn allows_double_colon_global_local_in_module_file() {
        // Written with `::` so they parse as pseudo-elements (Biome's fixture
        // exercises `:global`/`:local` as pseudo-classes via the sibling rule;
        // here we verify the pseudo-element path of the CSS-Modules carve-out).
        assert!(run_at("a::global { }", "s.module.css").is_empty());
        assert!(run_at("a::local { }", "s.module.css").is_empty());
    }

    #[test]
    fn flags_global_local_pseudo_element_in_plain_css() {
        // `::global` / `::local` are only valid in CSS Modules files.
        assert_eq!(run("a::global { }").len(), 1);
        assert_eq!(run("a::local { }").len(), 1);
    }

    #[test]
    fn allows_nested_after_before() {
        // https://github.com/biomejs/biome/issues/9081
        assert!(run("* {\n\t&::after,\n\t&::before {}\n}").is_empty());
    }

    // --- Additional AST-shape coverage. ---

    #[test]
    fn allows_class_and_id_and_type_selectors() {
        assert!(run(".foo { color: red; }").is_empty());
        assert!(run("#bar { color: red; }").is_empty());
        assert!(run("a:hover { }").is_empty());
        assert!(run("a:focus { }").is_empty());
    }

    #[test]
    fn does_not_flag_argument_of_functional_pseudo_element() {
        // `shadow-part` inside `::part(shadow-part)` is an argument, not a
        // pseudo-element name, and must not be validated.
        assert!(run("a::part(shadow-part) { }").is_empty());
    }

    // --- Biome `valid_with_ignore.css`: the `ignore` option. ---

    /// Run the check with `ignore = [...]` configured, exercising the real
    /// config-reading path (the default config has an empty `ignore` list).
    fn run_with_ignore(source: &str, ignore: &[&str]) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::rules::backend::{AstCheck, CheckCtx};
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let cfg_path = tmp.path().join("comply.toml");
        let ignore_toml = ignore
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(
            &cfg_path,
            format!("[rules.no-unknown-pseudo-element]\nignore = [{ignore_toml}]\n"),
        )
        .expect("write cfg");
        let cfg = Config::load_from(tmp.path()).expect("load cfg");

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_css::LANGUAGE.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let ctx = CheckCtx {
            path: std::path::Path::new("t.css"),
            path_arc: std::sync::Arc::from(std::path::Path::new("t.css")),
            source,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: crate::files::Language::Css,
        };
        Check.check(&ctx, &tree)
    }

    #[test]
    fn allows_ignored_unknown_pseudo_elements_case_insensitively() {
        let ignore = &[
            "custom-pseudo-element",
            "MyCustomPseudoElement",
            "another-custom-pseudo-element",
        ];
        assert!(run_with_ignore("a::custom-pseudo-element { }", ignore).is_empty());
        assert!(run_with_ignore("a::MyCustomPseudoElement { }", ignore).is_empty());
        assert!(run_with_ignore("a::mycustompseudoelement { }", ignore).is_empty());
        assert!(run_with_ignore("a::another-custom-pseudo-element { }", ignore).is_empty());
    }

    #[test]
    fn still_flags_unknown_not_in_ignore_list() {
        assert_eq!(
            run_with_ignore("a::unknown { }", &["custom-pseudo-element"]).len(),
            1
        );
    }
}
