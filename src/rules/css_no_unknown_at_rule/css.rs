//! Flag CSS at-rules whose name is not a standard CSS at-rule.
//!
//! tree-sitter-css parses widely-recognized at-rules into dedicated node kinds
//! (`charset_statement`, `media_statement`, `supports_statement`,
//! `import_statement`, `namespace_statement`, `keyframes_statement` — the last
//! also covers `@-webkit-keyframes`), and everything else into a generic
//! `at_rule` node carrying an `@name` `at_keyword`. We hook only `at_rule`, so
//! the dedicated-node at-rules are known by construction; for the generic node
//! we read the `at_keyword`, strip the leading `@`, and flag it when the name
//! is not in the standard set (case-insensitive, matching Biome). Reading the
//! name from the `at_keyword` token means an `@`-like substring inside a comment
//! or string never reaches this check. The macro visits nested `at_rule` nodes
//! too, so an unknown at-rule inside another at-rule's block is caught.

use crate::diagnostic::{Diagnostic, Severity};

/// Standard CSS at-rule names (without the leading `@`), lowercase. Covers the
/// top-level at-rules plus the page-margin box at-rules valid inside `@page`
/// and the font-feature value at-rules valid inside `@font-feature-values`.
const KNOWN_AT_RULES: &[&str] = &[
    // Top-level standard at-rules.
    "charset",
    "color-profile",
    "container",
    "counter-style",
    "document",
    "font-face",
    "font-feature-values",
    "font-palette-values",
    "function",
    "import",
    "keyframes",
    "layer",
    "media",
    "namespace",
    "page",
    "position-try",
    "property",
    "scope",
    "starting-style",
    "supports",
    "view-transition",
    // Page-margin box at-rules (valid inside `@page`).
    "top-left-corner",
    "top-left",
    "top-center",
    "top-right",
    "top-right-corner",
    "bottom-left-corner",
    "bottom-left",
    "bottom-center",
    "bottom-right",
    "bottom-right-corner",
    "left-top",
    "left-middle",
    "left-bottom",
    "right-top",
    "right-middle",
    "right-bottom",
    // Font-feature value at-rules (valid inside `@font-feature-values`).
    "annotation",
    "character-variant",
    "ornaments",
    "styleset",
    "stylistic",
    "swash",
];

crate::ast_check! { on ["at_rule"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(kw) = node.children(&mut c).find(|n| n.kind() == "at_keyword") else {
        return;
    };
    let Ok(keyword) = kw.utf8_text(source) else {
        return;
    };
    let name = keyword.trim_start_matches('@');
    if KNOWN_AT_RULES.iter().any(|known| name.eq_ignore_ascii_case(known)) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &kw,
        super::META.id,
        format!(
            "Unexpected unknown at-rule `@{name}`. It is not a standard CSS at-rule."
        ),
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

    #[test]
    fn flags_unknown_value_at_rule() {
        assert_eq!(run("@unknown-rule 'UTF-8';").len(), 1);
    }

    #[test]
    fn flags_unknown_block_at_rule() {
        assert_eq!(run("@unknown-at-rule {}").len(), 1);
    }

    #[test]
    fn flags_case_variants_of_unknown_name() {
        // `@uNkNoWn` and `@UNKNOWN` are both unknown regardless of case.
        assert_eq!(run("@uNkNoWn {}").len(), 1);
        assert_eq!(run("@UNKNOWN {}").len(), 1);
    }

    #[test]
    fn flags_nested_unknown_at_rule() {
        // The outer `@unknown` and the nested `@unknown-at-rule` both fire.
        let css = "@unknown { @unknown-at-rule { font-size: 14px; } }";
        assert_eq!(run(css).len(), 2);
    }

    #[test]
    fn flags_full_biome_invalid_fixture() {
        let css = "@unknown-rule 'UTF-8';\n\
@uNkNoWn {}\n\
@UNKNOWN {}\n\
@unknown-at-rule {}\n\
@unknown { @unknown-at-rule { font-size: 14px; } }\n\
@MY-other-at-rule {}\n\
@not-my-at-rule {}\n";
        assert_eq!(run(css).len(), 8);
    }

    #[test]
    fn allows_conditional_group_at_rules() {
        assert!(run("@media print { body { font-size: 10pt } }").is_empty());
        assert!(run("@supports (--foo: green) { body { color: green } }").is_empty());
        assert!(run("@container (min-width: 700px) {}").is_empty());
    }

    #[test]
    fn allows_statement_at_rules() {
        assert!(run("@charset 'UTF-8';").is_empty());
        assert!(run("@import 'custom.css';").is_empty());
        assert!(run("@namespace url(http://www.w3.org/1999/xhtml);").is_empty());
        assert!(run("@layer framework { h1 { background: white } }").is_empty());
    }

    #[test]
    fn allows_keyframes_and_vendor_prefixed_keyframes() {
        assert!(run("@keyframes id { 0% { top: 0 } }").is_empty());
        assert!(run("@-webkit-keyframes id { 0% { top: 0 } }").is_empty());
    }

    #[test]
    fn allows_block_at_rules() {
        assert!(run("@font-face { font-family: X }").is_empty());
        assert!(run("@counter-style win-list { system: fixed }").is_empty());
        assert!(run("@page :left { margin-left: 4cm }").is_empty());
        assert!(run("@property --foo {}").is_empty());
        assert!(run("@starting-style { opacity: 0 }").is_empty());
        assert!(run("@position-try --foo {}").is_empty());
        assert!(run("@view-transition { navigation: auto }").is_empty());
        assert!(run("@function --test-fn() { result: 1 }").is_empty());
        assert!(run("@document url(http://www.w3.org/) {}").is_empty());
    }

    #[test]
    fn allows_nested_page_margin_at_rule() {
        assert!(run("@page { @top-center { content: none } }").is_empty());
    }

    #[test]
    fn allows_nested_font_feature_value_at_rule() {
        let css = "@font-feature-values Font One { @styleset { nice-style: 12 } }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ignores_at_like_token_in_string() {
        assert!(run(".a { content: \"@madeup x\"; }").is_empty());
    }

    #[test]
    fn ignores_at_like_token_in_comment() {
        assert!(run("/* @alsofake {} */ .a { color: red }").is_empty());
    }

    #[test]
    fn allows_full_biome_valid_fixture() {
        let css = r#"@starting-style {
    opacity: 0;
}
@charset 'UTF-8';
@container (min-width: 700px) {}
@namespace url(http://www.w3.org/1999/xhtml);
@media print {
    body { font-size: 10pt }
}
@supports (--foo: green) {
    body { color: green; }
}
@counter-style win-list {
    system: fixed;
}
@page :left {
    margin-left: 4cm;
}
@page {
    @top-center { content: none }
}
@font-face {
    font-family: MyHelvetica;
}
@keyframes identifier {
    0% { top: 0; left: 0; }
}
@-webkit-keyframes identifier {
    0% { top: 0; left: 0; }
}
@font-feature-values Font One {
    @styleset { nice-style: 12; }
}
@layer framework {
    h1 { background: white; }
}
@position-try --foo {}
@view-transition {
    navigation: auto;
}
@function --test-fn() {
    result: 1;
}
"#;
        assert!(run(css).is_empty());
    }
}
