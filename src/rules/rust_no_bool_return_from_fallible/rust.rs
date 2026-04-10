//! rust-no-bool-return-from-fallible backend.
//!
//! Walks `function_item` nodes whose return type is `bool` and whose
//! name suggests an action (verb prefix from a small allowlist).
//! Pure predicates like `is_empty` / `has_x` / `contains` are
//! exempted: a bool is exactly what they should return.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const ACTION_PREFIXES: &[&str] = &[
    "save_", "delete_", "remove_", "create_", "update_", "insert_",
    "parse_", "validate_", "connect_", "send_", "write_", "load_",
    "execute_", "process_", "publish_", "submit_", "commit_",
    "apply_", "fetch_", "store_", "register_", "unregister_",
];

const EXEMPT_PREFIXES: &[&str] = &[
    "is_", "has_", "should_", "can_", "may_", "must_", "needs_",
    "contains_", "matches_", "supports_", "accepts_",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "function_item" {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            let Ok(name) = name_node.utf8_text(source_bytes) else {
                return;
            };
            if !looks_like_action(name) {
                return;
            }
            // Predicate-style names take precedence: an `is_valid()` that
            // returns bool is correct, even if it's also "save_valid".
            if looks_like_predicate(name) {
                return;
            }
            let Some(ret_type) = node.child_by_field_name("return_type") else {
                return;
            };
            let Ok(ret_text) = ret_type.utf8_text(source_bytes) else {
                return;
            };
            if ret_text.trim() != "bool" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-bool-return-from-fallible".into(),
                message: format!(
                    "`fn {name}(..) -> bool` — action functions must \
                     return `Result<T, E>` so the caller can see why \
                     the operation failed. Use `Result<(), MyError>` \
                     if there's no success payload."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn looks_like_action(name: &str) -> bool {
    let lower = format!("{}_", name.to_ascii_lowercase());
    ACTION_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn looks_like_predicate(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    EXEMPT_PREFIXES.iter().any(|p| lower.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_save_returning_bool() {
        assert_eq!(run_on("fn save_user(u: &User) -> bool { true }").len(), 1);
    }

    #[test]
    fn flags_parse_returning_bool() {
        assert_eq!(run_on("fn parse_config(s: &str) -> bool { true }").len(), 1);
    }

    #[test]
    fn allows_save_returning_result() {
        assert!(run_on("fn save_user(u: &User) -> Result<(), MyError> { Ok(()) }").is_empty());
    }

    #[test]
    fn allows_predicate_is_valid() {
        assert!(run_on("fn is_valid(s: &str) -> bool { true }").is_empty());
    }

    #[test]
    fn allows_predicate_has_permission() {
        assert!(run_on("fn has_permission(u: &User) -> bool { true }").is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_function() {
        assert!(run_on("fn add(a: i32, b: i32) -> i32 { a + b }").is_empty());
    }
}
