use crate::diagnostic::{Diagnostic, Severity};

// Limit the number of class selectors in a single selector, mirroring Biome's
// `noExcessiveSelectorClasses` (`@c2fd653`).
//
// Each selector in a comma-separated selector list is evaluated separately:
// `.foo, .bar.baz` is two selectors, only `.bar.baz` contributes two classes.
// Descendant/child combinators do not reset the count — `.foo .bar` is one
// selector with two classes. Class selectors nested in functional pseudo-class
// arguments (`:is(.foo, .bar)`, `:not(.foo.bar)`, `:nth-child(... of .foo)`)
// count toward the enclosing selector and are not reported on their own.
// Nested selectors are checked as written: `&.bar` contributes one class.

/// Count the class selectors in one selector subtree: every `class_name` node
/// whose parent is a `class_selector` (real `.foo` classes), excluding the
/// `class_name` children of `pseudo_class_selector` nodes (pseudo-class names
/// such as `is` / `not` / `nth-child`).
fn count_classes(selector: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = selector.walk();
    let mut stack = vec![selector];
    while let Some(node) = stack.pop() {
        if node.kind() == "class_name"
            && node.parent().is_some_and(|p| p.kind() == "class_selector")
        {
            count += 1;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

/// Whether this selector list was written with SCSS interpolation (`#{...}`).
/// tree-sitter-css cannot parse interpolation, leaving an `ERROR` node next to
/// the `selectors` node (before the rule's `block`). The final selector can't
/// be determined statically, so the whole list is skipped, matching stylelint.
fn has_interpolated_selector(selectors: tree_sitter::Node) -> bool {
    let Some(rule_set) = selectors.parent() else {
        return false;
    };
    let mut cursor = rule_set.walk();
    for child in rule_set.children(&mut cursor) {
        if child.kind() == "block" {
            break;
        }
        if child.is_error() {
            return true;
        }
    }
    false
}

fn selector_label(count: usize) -> &'static str {
    if count == 1 { "selector" } else { "selectors" }
}

crate::ast_check! { on ["selectors"] => |node, source, ctx, diagnostics|
    if has_interpolated_selector(node) {
        return;
    }

    let max = ctx
        .config
        .threshold("no-excessive-selector-classes", "max", ctx.lang);

    let mut cursor = node.walk();
    for selector in node.children(&mut cursor) {
        // Skip comma separators and any stray tokens; only named selector
        // nodes carry classes.
        if !selector.is_named() {
            continue;
        }
        let count = count_classes(selector);
        if count <= max {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &selector,
            super::META.id,
            format!(
                "Expected this selector to have no more than {max} class {}, but found {count}.",
                selector_label(max)
            ),
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

    /// Run against the default config (`max = 3`).
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    /// Run with a caller-supplied `max`, exercising the real config-reading
    /// path so the custom-threshold branch (and Biome's `maxClasses` fixtures)
    /// is covered.
    fn run_with_max(source: &str, max: usize) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::rules::backend::{AstCheck, CheckCtx};
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let cfg_path = tmp.path().join("comply.toml");
        std::fs::write(
            &cfg_path,
            format!("[rules.no-excessive-selector-classes]\nmax = {max}\n"),
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

    // --- Pure counting helper (independent of the threshold). ---

    fn count_first_selector(source: &str) -> usize {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_css::LANGUAGE.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let mut stack = vec![tree.root_node()];
        let mut cursor = tree.root_node().walk();
        while let Some(n) = stack.pop() {
            if n.kind() == "selectors" {
                let first = n.named_child(0).expect("selector");
                return count_classes(first);
            }
            for c in n.children(&mut cursor) {
                stack.push(c);
            }
        }
        panic!("no selectors node");
    }

    #[test]
    fn counts_chained_classes() {
        assert_eq!(count_first_selector(".foo {}"), 1);
        assert_eq!(count_first_selector(".foo.bar.baz {}"), 3);
        assert_eq!(count_first_selector("div.foo.bar {}"), 2);
    }

    #[test]
    fn descendant_combinator_does_not_reset() {
        assert_eq!(count_first_selector(".foo .bar {}"), 2);
        assert_eq!(count_first_selector(".ab .cd .ef {}"), 3);
    }

    #[test]
    fn pseudo_class_arguments_count_toward_selector() {
        assert_eq!(count_first_selector(":is(.foo, .bar.baz) {}"), 3);
        assert_eq!(count_first_selector(":not(.foo.bar) {}"), 2);
        assert_eq!(count_first_selector(":has(.foo .bar) {}"), 2);
        assert_eq!(count_first_selector(":nth-child(2n of .foo.bar) {}"), 2);
    }

    #[test]
    fn pseudo_class_name_is_not_a_class() {
        assert_eq!(count_first_selector("#id[attr]:hover::before {}"), 0);
        assert_eq!(count_first_selector(":nth-child(2n) {}"), 0);
    }

    // --- Biome `valid.css` (no options ⇒ off; here under default max = 3). ---

    #[test]
    fn default_allows_up_to_three_classes() {
        assert!(run("div {}").is_empty());
        assert!(run(".foo {}").is_empty());
        assert!(run("#id[attr]:hover::before {}").is_empty());
        assert!(run(":nth-child(2n) {}").is_empty());
        assert!(run(".foo.bar.baz {}").is_empty());
    }

    #[test]
    fn default_flags_four_classes() {
        assert_eq!(run(".foo.bar.baz.qux {}").len(), 1);
    }

    // --- Biome `invalid.css` (maxClasses = 1). ---

    #[test]
    fn max1_flags_descendant_pair() {
        assert_eq!(run_with_max(".foo .bar {}", 1).len(), 1);
    }

    #[test]
    fn max1_flags_only_the_offending_selector_in_a_list() {
        // `.foo` (1) is allowed, `.bar.baz` (2) fires.
        assert_eq!(run_with_max(".foo, .bar.baz {}", 1).len(), 1);
    }

    #[test]
    fn max1_flags_tag_with_two_classes() {
        assert_eq!(run_with_max("div.foo.bar {}", 1).len(), 1);
    }

    // --- Biome `valid.max2.css` (maxClasses = 2). ---

    #[test]
    fn max2_valid_fixtures() {
        assert!(run_with_max(".ab {}", 2).is_empty());
        assert!(run_with_max(".ab.cd {}", 2).is_empty());
        assert!(run_with_max(".ab .cd {}", 2).is_empty());
        assert!(run_with_max(".ab,\n.cd {}", 2).is_empty());
        assert!(run_with_max(".ab.cd,\n.ef.gh {}", 2).is_empty());
        assert!(run_with_max(".ab.cd[disabled]:hover {}", 2).is_empty());
        assert!(run_with_max(".ab { .cd {} }", 2).is_empty());
        assert!(run_with_max(".ab { .cd > & {} }", 2).is_empty());
        assert!(run_with_max(".ab, .cd { & > .ef {} }", 2).is_empty());
        assert!(run_with_max(".ab { &:hover > .ef.gh {} }", 2).is_empty());
        assert!(run_with_max("@media print { .ab.cd {} }", 2).is_empty());
        assert!(run_with_max(".ab { @media print { .cd {} } }", 2).is_empty());
    }

    // --- Biome `invalid.max2.css` (maxClasses = 2). ---

    #[test]
    fn max2_invalid_fixtures() {
        assert_eq!(run_with_max(".ab.cd.ef {}", 2).len(), 1);
        assert_eq!(run_with_max(":not(.ab.cd.ef) {}", 2).len(), 1);
        assert_eq!(run_with_max(".ab.cd :not(.ef.gh) {}", 2).len(), 1);
        assert_eq!(run_with_max(".ab .cd .ef {}", 2).len(), 1);
        assert_eq!(run_with_max(".ab,\n.cd.ef.gh {}", 2).len(), 1);
        assert_eq!(run_with_max(".ab.cd.ef :not(.gh) {}", 2).len(), 1);
    }

    // --- Biome `invalid.pseudo.css` (maxClasses = 1). ---

    #[test]
    fn max1_pseudo_fixtures() {
        assert_eq!(run_with_max(":is(.foo, .bar.baz) {}", 1).len(), 1);
        assert_eq!(run_with_max(":not(.foo.bar) {}", 1).len(), 1);
        assert_eq!(run_with_max(":has(.foo .bar) {}", 1).len(), 1);
        assert_eq!(run_with_max(":nth-child(2n of .foo.bar) {}", 1).len(), 1);
    }

    // --- Biome `invalid.nested.css` (maxClasses = 0). ---

    #[test]
    fn max0_nested_fixtures() {
        assert_eq!(run_with_max(".foo {\n  &.bar {}\n}", 0).len(), 2);
        assert_eq!(run_with_max(".foo, .bar {\n  &.baz {}\n}", 0).len(), 3);
    }

    // --- Biome `invalid.zero.css` / `valid.zero.css` (maxClasses = 0). ---

    #[test]
    fn max0_flags_any_class() {
        assert_eq!(run_with_max(".foo {}", 0).len(), 1);
        assert_eq!(run_with_max("div.foo {}", 0).len(), 1);
    }

    #[test]
    fn max0_allows_class_free_selectors() {
        assert!(run_with_max(":root { --foo: 1px; }", 0).is_empty());
        assert!(run_with_max("html { --foo: 1px; }", 0).is_empty());
    }

    // --- Biome `valid.zero.scss` / `invalid.zero.scss`: SCSS interpolation. ---

    #[test]
    fn max0_skips_interpolated_selectors() {
        assert!(run_with_max(".foo #{$test} {}", 0).is_empty());
        assert!(run_with_max(".foo.bar #{$test} {}", 0).is_empty());
    }

    #[test]
    fn max0_flags_scss_nested_class() {
        // `@include test { .foo {} }`: `.foo` is a plain class, no
        // interpolation, so it fires at max = 0.
        assert_eq!(run_with_max("@include test { .foo {} }", 0).len(), 1);
    }

    #[test]
    fn interpolation_in_declaration_value_does_not_skip_selector() {
        // Interpolation in a property value must not exempt the selector.
        assert_eq!(run_with_max(".foo { color: #{$x}; }", 0).len(), 1);
    }
}
