//! Flag the CSS Modules `@value` at-rule.
//!
//! `@value` is a CSS Modules construct (`@value primary: #fff;` and the import
//! form `@value a, b from "./x.css";`). tree-sitter-css parses the declaration
//! form as an `at_rule` node and the import form as an `ERROR` node; both carry
//! an `@value` `at_keyword` as their first child, so we iterate the direct
//! children of `stylesheet` rather than relying on a single node kind. The
//! `at_keyword` token never appears inside comments, strings, or property
//! names, so a `value` property or a comment mentioning "value" stays clean.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["stylesheet"] => |node, source, ctx, diagnostics|
    if !is_css_module(ctx.path) {
        return;
    }
    let mut c = node.walk();
    for kid in node.children(&mut c) {
        if !is_value_at_rule(&kid, source) {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &kid,
            super::META.id,
            "Unexpected `@value` at-rule. Use a CSS custom property instead.".to_string(),
            Severity::Warning,
        ));
    }
}

/// Biome scopes this rule to CSS Modules files; mirror that by gating on the
/// `.module.css` extension.
fn is_css_module(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".module.css"))
}

/// True when this `stylesheet` child is a `@value` at-rule. Both the
/// declaration form (`at_rule`) and the import form (`ERROR`) start with an
/// `@value` `at_keyword`.
fn is_value_at_rule(node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut c = node.walk();
    node.children(&mut c)
        .find(|n| n.kind() == "at_keyword")
        .and_then(|n| n.utf8_text(source).ok())
        .is_some_and(|kw| kw.eq_ignore_ascii_case("@value"))
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
        crate::rules::test_helpers::run_rule(&Check, s, "styles.module.css")
    }

    #[test]
    fn flags_simple_value() {
        let css = "@value primary: #BF4040;";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_media_query_value() {
        let css = "@value small: (max-width: 599px);";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_string_value() {
        let css = "@value colors: \"./colors.css\";";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_import_form() {
        let css = "@value primary, secondary from colors;";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_import_with_alias() {
        let css = "@value small as bp-small, large as bp-large from \"./breakpoints.css\";";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_biome_invalid_fixture() {
        // The full invalid.module.css fixture from Biome — nine @value at-rules.
        let css = r#"/* css variables */
@value primary: #BF4040;
@value secondary: #1F4F7F;

/* breakpoints */
@value small: (max-width: 599px);
@value medium: (min-width: 600px) and (max-width: 959px);
@value large: (min-width: 960px);

/* alias paths for other values or composition */
@value colors: "./colors.css";
/* import multiple from a single file */
@value primary, secondary from colors;
/* make local aliases to imported values */
@value small as bp-small, large as bp-large from "./breakpoints.css";
/* value as selector name */
@value selectorValue: secondary-color;
"#;
        assert_eq!(run(css).len(), 9);
    }

    #[test]
    fn allows_biome_valid_fixture() {
        let css = r#"/* should not generate diagnostics */
@import "./colors.module.css";
:root {
    --main-color: red;
}
"#;
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_import_media_supports() {
        let css = "@import \"x.css\"; @media (min-width: 1px) { a { color: red } } @supports (display: grid) { a { color: red } }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_custom_property() {
        let css = ":root { --primary: #fff; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_value_property() {
        let css = ".a { value: 1; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ignores_value_inside_string() {
        let css = ".a { content: \"@value primary: red\"; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ignores_value_inside_comment() {
        let css = "/* @value primary: #fff; */ .a { color: red; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn skips_non_module_css() {
        let css = "@value primary: #BF4040;";
        let diags = crate::rules::test_helpers::run_rule(&Check, css, "styles.css");
        assert!(diags.is_empty());
    }
}
