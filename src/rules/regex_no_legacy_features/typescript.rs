//! regex-no-legacy-features TypeScript / JavaScript / TSX backend.
//!
//! Flags uses of legacy `RegExp` static properties (`RegExp.$1`-`$9`,
//! `RegExp.lastMatch`, etc.) via AST member access / subscript
//! detection. Using the AST eliminates FPs from these identifiers
//! appearing inside strings or comments.

use crate::diagnostic::{Diagnostic, Severity};

const LEGACY_PROPS: &[&str] = &[
    "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9",
    "lastMatch", "lastParen", "leftContext", "rightContext", "input",
    "$_", "$&", "$+", "$`", "$'",
];

fn is_regexp_ident(node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    node.kind() == "identifier"
        && node.utf8_text(source).map(|t| t == "RegExp").unwrap_or(false)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // member_expression: RegExp.$1 / RegExp.lastMatch / RegExp.$_
    if node.kind() == "member_expression"
        && let Some(obj) = node.child_by_field_name("object")
        && is_regexp_ident(&obj, source)
        && let Some(prop) = node.child_by_field_name("property")
        && let Ok(name) = prop.utf8_text(source)
        && LEGACY_PROPS.contains(&name)
    {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "regex-no-legacy-features",
            "Avoid legacy RegExp static properties \u{2014} use capturing groups and match results instead.".into(),
            Severity::Warning,
        ));
        return;
    }

    // subscript_expression: RegExp["$&"], RegExp["$_"], etc.
    if node.kind() == "subscript_expression"
        && let Some(obj) = node.child_by_field_name("object")
        && is_regexp_ident(&obj, source)
        && let Some(idx) = node.child_by_field_name("index")
        && idx.kind() == "string"
        && let Ok(raw) = idx.utf8_text(source)
    {
        let trimmed = raw.trim_start_matches(['"', '\'', '`']).trim_end_matches(['"', '\'', '`']);
        if LEGACY_PROPS.contains(&trimmed) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "regex-no-legacy-features",
                "Avoid legacy RegExp static properties \u{2014} use capturing groups and match results instead.".into(),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_regexp_dollar1() {
        assert_eq!(run_on(r#"const x = RegExp.$1;"#).len(), 1);
    }

    #[test]
    fn flags_regexp_lastmatch() {
        assert_eq!(run_on(r#"const x = RegExp.lastMatch;"#).len(), 1);
    }

    #[test]
    fn flags_regexp_subscript() {
        assert_eq!(run_on(r#"const x = RegExp["$&"];"#).len(), 1);
    }

    #[test]
    fn allows_normal_regexp_usage() {
        assert!(run_on(r#"const re = new RegExp("foo");"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_string_containing_regexp_legacy_syntax() {
        let src = r#"const doc = "Use RegExp.$1 for legacy match";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
