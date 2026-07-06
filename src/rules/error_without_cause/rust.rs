//! error-without-cause Rust backend.
//!
//! Flags patterns like `anyhow!("{}", e.to_string())` or `bail!(Error::Http(e.to_string()))`
//! that stringify a caught error into a fresh error without preserving the source
//! via `.context()` or `.source()`. In Rust the idiomatic pattern is `.context("msg")`
//! or wrapping with `#[from]`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "anyhow" && mac_name != "bail" {
        return;
    }

    let Ok(full_text) = node.utf8_text(source) else { return };

    // A `.context()`/`source`/`cause` in the macro or its enclosing statement means
    // the cause is already preserved (e.g. `anyhow!(..).context(..)`).
    let parent_text = node
        .parent()
        .and_then(|p| p.utf8_text(source).ok())
        .unwrap_or("");
    let combined = if parent_text.is_empty() { full_text } else { parent_text };

    // Flag only when the macro stringifies a caught-error *binding* (`err.to_string()`,
    // a discarded cause) or reads an error `.message`. Building `String` fields of a
    // fresh structured error from a literal (`"name".to_string()`) or a struct field
    // (`self.ix.name.to_string()`) discards no cause and is not flagged.
    if (macro_stringifies_error_binding(node, source) || full_text.contains(".message"))
        && !combined.contains("source")
        && !combined.contains("context")
        && !combined.contains("cause")
    {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-without-cause".into(),
            message: "Error wraps message without preserving cause — use `.context()` or pass `source`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when the macro's arguments call `.to_string()` on a bare identifier — a
/// caught-error binding (`err`, `e`) whose cause is discarded into a `String`.
///
/// Macro arguments are an unparsed `token_tree` flat token stream, so
/// `err.to_string()` flattens to `(identifier "err") . (identifier "to_string")
/// (token_tree "()")`. The receiver is the token before the `.`; only a bare
/// identifier is a potential caught error.
fn macro_stringifies_error_binding(macro_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = macro_node.walk();
    macro_node
        .children(&mut cursor)
        .find(|child| child.kind() == "token_tree")
        .is_some_and(|token_tree| token_tree_stringifies_binding(token_tree, source))
}

/// Recursively scans a `token_tree` for a `.to_string()` call whose receiver is a
/// bare identifier. Nested groups (`Error::Http(err.to_string())`, `Error::Foo { .. }`)
/// are walked in turn.
fn token_tree_stringifies_binding(token_tree: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = token_tree.walk();
    let children: Vec<tree_sitter::Node> = token_tree.children(&mut cursor).collect();
    for (index, child) in children.iter().enumerate() {
        if child.kind() == "token_tree" {
            if token_tree_stringifies_binding(*child, source) {
                return true;
            }
            continue;
        }
        // `.to_string()`: a `to_string` identifier preceded by `.` and followed by a
        // `(` group.
        if child.kind() != "identifier" || child.utf8_text(source) != Ok("to_string") {
            continue;
        }
        let preceded_by_dot = index
            .checked_sub(1)
            .and_then(|i| children.get(i))
            .is_some_and(|n| n.utf8_text(source) == Ok("."));
        let followed_by_call = children
            .get(index + 1)
            .is_some_and(|n| source.get(n.start_byte()) == Some(&b'('));
        if preceded_by_dot && followed_by_call && receiver_is_bare_identifier(&children, index, source)
        {
            return true;
        }
    }
    false
}

/// Whether the receiver of the `.to_string()` at `to_string_index` (the token two
/// positions back, before the `.`) is a bare identifier rather than a literal, field
/// access, scoped path or call/index expression. A `string_literal` receiver is never
/// a discarded cause, and an identifier preceded by `.` (`self.ix.name`) or `::`
/// (`Type::CONST`) is a field/path segment, not a caught-error binding.
fn receiver_is_bare_identifier(
    children: &[tree_sitter::Node],
    to_string_index: usize,
    source: &[u8],
) -> bool {
    let Some(receiver) = to_string_index.checked_sub(2).and_then(|i| children.get(i)) else {
        return false;
    };
    if receiver.kind() != "identifier" {
        return false;
    }
    let preceded_by_access = to_string_index
        .checked_sub(3)
        .and_then(|i| children.get(i))
        .is_some_and(|n| matches!(n.utf8_text(source), Ok(".") | Ok("::")));
    !preceded_by_access
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_anyhow_with_to_string() {
        let src = r#"fn f(e: Error) { anyhow!("{}", e.to_string()); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_anyhow_with_context() {
        let src = r#"fn f(e: Error) { anyhow!("{}", e.to_string()).context("wrapping"); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_bail_wrapping_error_binding() {
        let src = r#"fn h() -> Result<()> { bail!(Error::Http(err.to_string())) }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_bail_struct_error_from_string_literals() {
        let src = r#"fn f() -> Result<()> {
            bail!(Error::InvalidFunctionArguments {
                name: "object::from_entries".to_string(),
                message: "Expected entries".to_string(),
            })
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bail_struct_error_from_field_access() {
        let src = r#"fn g() -> Result<()> {
            bail!(Error::IndexExists { record: rid, index: self.ix.name.to_string() })
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_mixed_literal_and_binding_to_string() {
        let src = r#"fn m() -> Result<()> {
            bail!(Error::Wrapped { label: "domain".to_string(), inner: err.to_string() })
        }"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
