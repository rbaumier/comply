use crate::diagnostic::{Diagnostic, Severity};

// Known pseudo-class / single-colon pseudo-element names, ported verbatim from
// Biome's `biome_css_analyze` keyword tables (`@c2fd653`). Each array is sorted
// so membership is a `binary_search`. Matching is case-insensitive: callers pass
// an already-lowercased name.

/// Pseudo-elements that accept single-colon notation, so `:before` etc. are
/// valid pseudo-classes too (`LEVEL_ONE_AND_TWO_PSEUDO_ELEMENTS`).
const LEVEL_ONE_AND_TWO_PSEUDO_ELEMENTS: &[&str] =
    &["after", "before", "first-letter", "first-line", "slotted"];

const A_NPLUS_BNOTATION_PSEUDO_CLASSES: &[&str] =
    &["nth-column", "nth-last-column", "nth-last-of-type", "nth-of-type"];

const A_NPLUS_BOF_SNOTATION_PSEUDO_CLASSES: &[&str] = &["nth-child", "nth-last-child"];

const LINGUISTIC_PSEUDO_CLASSES: &[&str] = &["dir", "lang"];

const LOGICAL_COMBINATIONS_PSEUDO_CLASSES: &[&str] = &["has", "is", "matches", "not", "where"];

const RESOURCE_STATE_PSEUDO_CLASSES: &[&str] = &[
    "buffering",
    "muted",
    "paused",
    "playing",
    "seeking",
    "stalled",
    "volume-locked",
];

const OTHER_PSEUDO_CLASSES: &[&str] = &[
    "active",
    "active-view-transition",
    "active-view-transition-type",
    "any-link",
    "autofill",
    "blank",
    "checked",
    "current",
    "default",
    "defined",
    "disabled",
    "empty",
    "enabled",
    "first-child",
    "first-of-type",
    "focus",
    "focus-visible",
    "focus-within",
    "fullscreen",
    "fullscreen-ancestor",
    "future",
    "has-slotted",
    "host",
    "host-context",
    "hover",
    "in-range",
    "indeterminate",
    "invalid",
    "last-child",
    "last-of-type",
    "link",
    "modal",
    "only-child",
    "only-of-type",
    "open",
    "optional",
    "out-of-range",
    "past",
    "picture-in-picture",
    "placeholder-shown",
    "popover-open",
    "read-only",
    "read-write",
    "required",
    "root",
    "scope",
    "state",
    "target",
    "target-after",
    "target-before",
    "target-current",
    "unresolved",
    "user-invalid",
    "user-valid",
    "valid",
    "visited",
    "window-inactive",
];

/// Pseudo-classes that are only valid when scoped to a `::-webkit-scrollbar*`
/// pseudo-element (`WEBKIT_SCROLLBAR_PSEUDO_CLASSES`).
const WEBKIT_SCROLLBAR_PSEUDO_CLASSES: &[&str] = &[
    "corner-present",
    "decrement",
    "double-button",
    "end",
    "horizontal",
    "increment",
    "no-button",
    "single-button",
    "start",
    "vertical",
    "window-inactive",
];

/// `::-webkit-scrollbar*` pseudo-elements that scope the scrollbar pseudo-classes
/// (`WEBKIT_SCROLLBAR_PSEUDO_ELEMENTS`). Names are stored without the leading
/// `::`, matching the tree-sitter `tag_name` text.
const WEBKIT_SCROLLBAR_PSEUDO_ELEMENTS: &[&str] = &[
    "-webkit-resizer",
    "-webkit-scrollbar",
    "-webkit-scrollbar-button",
    "-webkit-scrollbar-corner",
    "-webkit-scrollbar-thumb",
    "-webkit-scrollbar-track",
    "-webkit-scrollbar-track-piece",
];

/// CSS Modules pseudo-classes, valid only in `.module.css` files
/// (`CSS_MODULE_PSEUDO_CLASSES`).
const CSS_MODULE_PSEUDO_CLASSES: &[&str] = &["global", "local"];

/// `is_known_pseudo_class`: the union of every standard pseudo-class table.
fn is_known_pseudo_class(name: &str) -> bool {
    LEVEL_ONE_AND_TWO_PSEUDO_ELEMENTS.binary_search(&name).is_ok()
        || A_NPLUS_BNOTATION_PSEUDO_CLASSES.binary_search(&name).is_ok()
        || A_NPLUS_BOF_SNOTATION_PSEUDO_CLASSES.binary_search(&name).is_ok()
        || LINGUISTIC_PSEUDO_CLASSES.binary_search(&name).is_ok()
        || LOGICAL_COMBINATIONS_PSEUDO_CLASSES.binary_search(&name).is_ok()
        || RESOURCE_STATE_PSEUDO_CLASSES.binary_search(&name).is_ok()
        || OTHER_PSEUDO_CLASSES.binary_search(&name).is_ok()
}

/// `is_custom_selector`: a `--`-prefixed custom selector.
fn is_custom_selector(name: &str) -> bool {
    name.starts_with("--")
}

/// `vendor_prefixed`: a recognised vendor prefix.
fn vendor_prefixed(name: &str) -> bool {
    name.starts_with("-webkit-")
        || name.starts_with("-moz-")
        || name.starts_with("-ms-")
        || name.starts_with("-o-")
}

/// Whether this pseudo-class is scoped to a `::-webkit-scrollbar*` pseudo-element
/// in the same compound selector. tree-sitter-css nests compound selectors, so
/// the scrollbar pseudo-element is the leftmost descendant of the enclosing
/// `pseudo_class_selector`. Walk down the leading chain looking for it.
fn is_webkit_scrollbar_scoped(class_name: &tree_sitter::Node, source: &[u8]) -> bool {
    let Some(mut node) = class_name.parent() else {
        return false;
    };
    while node.kind() == "pseudo_class_selector" {
        let mut cursor = node.walk();
        let mut next: Option<tree_sitter::Node> = None;
        for child in node.children(&mut cursor) {
            match child.kind() {
                "pseudo_element_selector" => {
                    if pseudo_element_name(&child, source)
                        .is_some_and(|n| WEBKIT_SCROLLBAR_PSEUDO_ELEMENTS.binary_search(&n).is_ok())
                    {
                        return true;
                    }
                }
                "pseudo_class_selector" => next = Some(child),
                _ => {}
            }
        }
        match next {
            Some(child) => node = child,
            None => break,
        }
    }
    false
}

/// The pseudo-element name (`tag_name` text after `::`) of a
/// `pseudo_element_selector` node, e.g. `-webkit-scrollbar` for
/// `::-webkit-scrollbar`.
fn pseudo_element_name<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "tag_name")
        .and_then(|c| c.utf8_text(source).ok())
}

/// Biome scopes CSS Modules pseudo-classes to CSS Modules files; mirror that by
/// gating on the `.module.css` extension.
fn is_css_module(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".module.css"))
}

crate::ast_check! { on ["class_name"] => |node, source, ctx, diagnostics|
    // Only `class_name` nodes that name a pseudo-class (`a:hover` → `hover`),
    // not class selectors (`.foo`) or names nested inside `:not(...)` arguments.
    if node.parent().map(|p| p.kind()) != Some("pseudo_class_selector") {
        return;
    }
    let name = node.utf8_text(source).unwrap_or_default();
    // `:foo-#{$name}` (Sass interpolation) cannot be validated; tree-sitter
    // leaves a trailing `-`. Skip names that aren't a complete identifier.
    if name.is_empty() || name.contains('#') {
        return;
    }
    let lower = name.to_ascii_lowercase();
    let lower = lower.as_str();

    let scrollbar_scoped = is_webkit_scrollbar_scoped(&node, source);
    let is_valid = if scrollbar_scoped {
        WEBKIT_SCROLLBAR_PSEUDO_CLASSES.binary_search(&lower).is_ok()
            || is_known_pseudo_class(lower)
    } else {
        is_custom_selector(lower) || vendor_prefixed(lower) || is_known_pseudo_class(lower)
    };

    if is_valid {
        return;
    }
    let ignore = ctx
        .config
        .string_list("no-unknown-pseudo-class", "ignore", ctx.lang);
    if ignore.iter().any(|p| p.eq_ignore_ascii_case(name)) {
        return;
    }
    if is_css_module(ctx.path) && CSS_MODULE_PSEUDO_CLASSES.binary_search(&lower).is_ok() {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Unexpected unknown pseudo-class `{name}`."),
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
    fn allows_hover_case_insensitive() {
        assert!(run("a:hover { }").is_empty());
        assert!(run("a:Hover { }").is_empty());
        assert!(run("a:hOvEr { }").is_empty());
        assert!(run("a:HOVER { }").is_empty());
    }

    #[test]
    fn allows_focus_visible() {
        assert!(run("a:focus-visible { }").is_empty());
    }

    #[test]
    fn allows_single_colon_before() {
        assert!(run("a:before { }").is_empty());
        assert!(run("a::before { }").is_empty());
    }

    #[test]
    fn allows_modal_and_root() {
        assert!(run(":modal { }").is_empty());
        assert!(run(":root { }").is_empty());
    }

    #[test]
    fn allows_functional_not() {
        assert!(run("input:not([type='submit']) { }").is_empty());
    }

    #[test]
    fn allows_matches_and_has() {
        assert!(run(":matches(section, article, aside, nav) h1 { }").is_empty());
        assert!(run("a:has(> img) { }").is_empty());
        assert!(run("section:has(h1, h2, h3, h4, h5, h6) { }").is_empty());
    }

    #[test]
    fn allows_nested_functional_pseudo_classes() {
        assert!(run("p:has(img):not(:has(:not(img))) { }").is_empty());
        assert!(run("div.sidebar:has(*:nth-child(5)):not(:has(*:nth-child(6))) { }").is_empty());
    }

    #[test]
    fn allows_nth_child_of_notation() {
        assert!(run("div :nth-child(2 of .widget) { }").is_empty());
    }

    #[test]
    fn allows_pseudo_class_before_pseudo_element() {
        assert!(run("a:hover::before { }").is_empty());
    }

    #[test]
    fn allows_vendor_prefixed_pseudo_class() {
        assert!(run("a:-moz-placeholder { }").is_empty());
    }

    #[test]
    fn allows_selector_list_with_combinator() {
        assert!(run("a,\nb > .foo:hover { }").is_empty());
    }

    #[test]
    fn allows_custom_double_dash_selector() {
        assert!(run(":--heading { }").is_empty());
    }

    #[test]
    fn allows_webkit_scrollbar_scoped_pseudo_classes() {
        assert!(run("::-webkit-scrollbar-thumb:window-inactive { }").is_empty());
        assert!(run("::-webkit-scrollbar-button:horizontal:decrement {}").is_empty());
        assert!(run(".test::-webkit-scrollbar-button:horizontal:decrement {}").is_empty());
        assert!(run("::-webkit-scrollbar-button:hover {}").is_empty());
    }

    #[test]
    fn allows_window_inactive_on_selection() {
        // `window-inactive` is also a known pseudo-class, so it is valid even
        // when scoped to a non-scrollbar pseudo-element.
        assert!(run("::selection:window-inactive { }").is_empty());
    }

    #[test]
    fn allows_not_chains() {
        assert!(run("body:not(div):not(span) {}").is_empty());
    }

    #[test]
    fn allows_custom_properties() {
        assert!(run(":root { --foo: 1px; }").is_empty());
        assert!(run("html { --foo: 1px; }").is_empty());
    }

    #[test]
    fn allows_defined_and_is_universal() {
        assert!(run("a:defined { }").is_empty());
        assert!(run("*:is(*) { }").is_empty());
    }

    #[test]
    fn allows_popover_open() {
        assert!(run(":popover-open {}").is_empty());
    }

    #[test]
    fn allows_resource_state_pseudo_classes() {
        assert!(run(":seeking, :stalled, :buffering, :volume-locked, :muted {}").is_empty());
    }

    #[test]
    fn allows_open_state() {
        assert!(run("dialog:open {}").is_empty());
    }

    #[test]
    fn allows_state_functional_pseudo_class() {
        assert!(run("custom-selector:state(checked) {}").is_empty());
    }

    #[test]
    fn allows_active_view_transition() {
        assert!(run(":active-view-transition * { transition-duration: 0s; }").is_empty());
        assert!(run("html:active-view-transition-type(slide) {}").is_empty());
    }

    #[test]
    fn allows_target_current_on_pseudo_element() {
        assert!(run("::scroll-marker:target-current {}").is_empty());
        assert!(run("::scroll-marker:target-before {}").is_empty());
        assert!(run("::scroll-marker:target-after {}").is_empty());
    }

    #[test]
    fn allows_has_slotted() {
        assert!(run("slot:has-slotted {}").is_empty());
    }

    // --- Biome `invalid.css`: must fire. ---

    #[test]
    fn flags_unknown_case_insensitive() {
        assert_eq!(run("a:unknown { }").len(), 1);
        assert_eq!(run("a:Unknown { }").len(), 1);
        assert_eq!(run("a:uNkNoWn { }").len(), 1);
        assert_eq!(run("a:UNKNOWN { }").len(), 1);
    }

    #[test]
    fn flags_pseudo_class_literal_name() {
        assert_eq!(run("a:pseudo-class { }").len(), 1);
    }

    #[test]
    fn flags_unknown_in_not_chain() {
        // `:not` is valid, `:noot` fires.
        assert_eq!(run("body:not(div):noot(span) {}").len(), 1);
    }

    #[test]
    fn flags_unknown_before_pseudo_element() {
        assert_eq!(run("a:unknown::before { }").len(), 1);
    }

    #[test]
    fn flags_unknown_in_selector_list() {
        assert_eq!(run("a,\nb > .foo:error { }").len(), 1);
    }

    #[test]
    fn flags_unknown_after_scrollbar_scoped_pseudo_class() {
        // `:horizontal` is valid (scrollbar-scoped), `:unknown` fires.
        assert_eq!(
            run("::-webkit-scrollbar-button:horizontal:unknown {}").len(),
            1
        );
    }

    #[test]
    fn flags_bare_first() {
        // `:first` is only valid as an `@page` pseudo-class; bare it fires.
        assert_eq!(run(":first { }").len(), 1);
    }

    #[test]
    fn flags_placeholder_pseudo_class() {
        // `placeholder` is a pseudo-element, not a pseudo-class.
        assert_eq!(run(":placeholder {}").len(), 1);
    }

    #[test]
    fn flags_scrollbar_pseudo_classes_without_scrollbar_element() {
        // Not scoped to a `::-webkit-scrollbar*` element, so both fire.
        assert_eq!(run(":horizontal:decrement {}").len(), 2);
    }

    // --- Biome `validGlobal.module.css`: CSS Modules `:global` / `:local`. ---

    #[test]
    fn allows_css_module_pseudo_classes_in_module_file() {
        assert!(run_at(".meow :global(.global) { color: blue; }", "s.module.css").is_empty());
        assert!(run_at(".foo :local(.bar) { color: red; }", "s.module.css").is_empty());
        assert!(run_at(":local(.component) { margin: 0; }", "s.module.css").is_empty());
        assert!(
            run_at(".parent :local(.child):hover { background: blue; }", "s.module.css").is_empty()
        );
    }

    #[test]
    fn flags_css_module_pseudo_classes_in_plain_css() {
        // `:global` / `:local` are only valid in CSS Modules files.
        assert_eq!(run("a:global(.foo) {}").len(), 1);
    }

    // --- Additional AST-shape coverage. ---

    #[test]
    fn allows_class_selector() {
        assert!(run(".foo { color: red; }").is_empty());
    }

    #[test]
    fn allows_id_selector() {
        assert!(run("#bar { color: red; }").is_empty());
    }

    #[test]
    fn does_not_flag_inner_selector_of_functional_pseudo_class() {
        // `.widget` inside `:nth-child(2 of .widget)` is a class, not a
        // pseudo-class, and must not be validated.
        assert!(run("div :nth-child(2 of .widget) { }").is_empty());
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
            format!("[rules.no-unknown-pseudo-class]\nignore = [{ignore_toml}]\n"),
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
    fn allows_ignored_unknown_pseudo_classes_case_insensitively() {
        let ignore = &[
            "custom-pseudo-class",
            "MyCustomPseudoClass",
            "another-custom-pseudo-class",
        ];
        assert!(run_with_ignore("a:custom-pseudo-class { }", ignore).is_empty());
        assert!(run_with_ignore("a:MyCustomPseudoClass { }", ignore).is_empty());
        assert!(run_with_ignore("a:mycustompseudoclass { }", ignore).is_empty());
        assert!(run_with_ignore("a:another-custom-pseudo-class { }", ignore).is_empty());
    }

    #[test]
    fn still_flags_unknown_not_in_ignore_list() {
        assert_eq!(
            run_with_ignore("a:unknown { }", &["custom-pseudo-class"]).len(),
            1
        );
    }
}
