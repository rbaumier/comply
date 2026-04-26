//! migration-needs-rollback — Rust backend.
//!
//! A Rust migration is identified by a `fn up(...)` declaration. If
//! `fn up` exists but no `fn down` / `fn rollback` exists in the same
//! file, the migration is one-way. Walks `function_item` nodes via
//! the AST so identifiers like `setup` or `lookup_user` don't trigger.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut has_up = false;
        let mut has_down = false;
        for node in collect_nodes_of_kinds(tree, &["function_item"]) {
            let Some(name_node) = node.child_by_field_name("name") else {
                continue;
            };
            let Ok(name) = name_node.utf8_text(source) else {
                continue;
            };
            if name == "up" {
                has_up = true;
            } else if name == "down" || name == "rollback" {
                has_down = true;
            }
        }
        if has_up && !has_down {
            return vec![Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: "migration-needs-rollback".into(),
                message: "Migration has `up()` but no `down()` / rollback — every migration must be reversible for quick recovery from bad deploys.".into(),
                severity: Severity::Warning,
                span: None,
            }];
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_up_without_down() {
        let src = "fn up() { println!(\"create table\"); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_up_with_down() {
        let src = "fn up() {} fn down() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_up_with_rollback() {
        let src = "fn up() {} fn rollback() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_migration() {
        let src = "fn do_stuff() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_setup_and_lookup() {
        let src = "fn setup() {} fn lookup_user() {}";
        assert!(run(src).is_empty());
    }
}
