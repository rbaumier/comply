//! regex-no-missing-g-flag TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. A regex literal is flagged when
//! it is passed as an argument to `<expr>.matchAll(...)` or
//! `<expr>.replaceAll(...)` and its flag list does not contain `g`.
//!
//! Gating by AST eliminates the false-positive class from the previous
//! TextCheck (which matched regex-like substrings inside Tailwind classes,
//! URLs and scoped import paths such as `"@scope/pkg"`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Methods whose regex argument must carry the `g` flag for correctness.
const G_REQUIRED_METHODS: &[&str] = &["matchAll", "replaceAll"];

/// Returns the containing `call_expression` node when `node` is an
/// argument of a call, skipping any intermediate `arguments` wrapper
/// and `parenthesized_expression` nodes.
fn containing_call<'t>(node: tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    let mut current = node.parent()?;
    // `/regex/` may be wrapped in `parenthesized_expression` before the
    // `arguments` list — e.g. `foo.matchAll((/re/))`.
    while current.kind() == "parenthesized_expression" {
        current = current.parent()?;
    }
    if current.kind() != "arguments" {
        return None;
    }
    let call = current.parent()?;
    if call.kind() != "call_expression" {
        return None;
    }
    Some(call)
}

/// Returns the property name of `<object>.<property>(...)` call, or `None`
/// if the callee is not a member expression.
fn called_method_name<'a>(call: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "member_expression" {
        return None;
    }
    let prop = func.child_by_field_name("property")?;
    prop.utf8_text(source).ok()
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((_pattern, flags)) = pattern_and_flags(&node, source) else { return };
    if flags.contains('g') {
        return;
    }
    let Some(call) = containing_call(node) else { return };
    let Some(method) = called_method_name(call, source) else { return };
    if !G_REQUIRED_METHODS.contains(&method) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-missing-g-flag",
        "Regex passed to a method that requires the `g` flag but it is missing.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_matchall_without_g() {
        assert_eq!(run_on(r#"str.matchAll(/foo/i);"#).len(), 1);
    }

    #[test]
    fn allows_matchall_with_g() {
        assert!(run_on(r#"str.matchAll(/foo/gi);"#).is_empty());
    }

    #[test]
    fn flags_replaceall_without_g() {
        assert_eq!(run_on(r#"str.replaceAll(/bar/, "baz");"#).len(), 1);
    }

    #[test]
    fn allows_replaceall_with_g() {
        assert!(run_on(r#"str.replaceAll(/bar/g, "baz");"#).is_empty());
    }

    #[test]
    fn allows_replace_without_g() {
        // `.replace(...)` does not require the `g` flag.
        assert!(run_on(r#"str.replace(/bar/, "baz");"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
