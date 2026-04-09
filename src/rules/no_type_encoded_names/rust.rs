//! no-type-encoded-names backend for Rust.
//!
//! Flags identifiers that encode their type in the name Hungarian-style:
//! `str_name`, `arr_items`, `bool_flag`, `i_count`. Rust's type system
//! already knows the type — the prefix is redundant and lies when the
//! type changes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const TYPE_PREFIXES: &[&str] = &[
    "str", "arr", "obj", "num", "bool", "int", "fn", "func", "vec",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "identifier" {
                return;
            }
            if !is_declaration_site(node) {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            let Some(prefix) = matched_type_prefix(name) else {
                return;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-type-encoded-names".into(),
                message: format!(
                    "'{name}' encodes a type prefix '{prefix}' — Hungarian \
                     notation is obsolete. Remove the prefix; the type \
                     system already tells you the type."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "let_declaration" | "parameter" | "function_item" | "const_item" | "static_item"
    )
}

/// Return the type prefix matched at a snake_case word boundary.
/// `str_name` → Some("str"), `strawberry` → None (no underscore after "str").
fn matched_type_prefix(name: &str) -> Option<&'static str> {
    for &prefix in TYPE_PREFIXES {
        if name.starts_with(&format!("{prefix}_")) {
            return Some(prefix);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_str_prefix() {
        assert_eq!(run_on("fn f() { let str_name = String::new(); }").len(), 1);
    }

    #[test]
    fn flags_arr_prefix() {
        assert_eq!(run_on("fn f() { let arr_items = vec![]; }").len(), 1);
    }

    #[test]
    fn flags_bool_prefix() {
        assert_eq!(run_on("fn f() { let bool_flag = true; }").len(), 1);
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("fn f() { let user_name = String::new(); }").is_empty());
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // `string` and `array` start with str/arr but without underscore.
        assert!(run_on("fn f() { let strawberry = 1; }").is_empty());
        assert!(run_on("fn f() { let array_of_things = vec![]; }").is_empty());
    }
}
