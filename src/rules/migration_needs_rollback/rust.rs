//! migration-needs-rollback — Rust backend.
//!
//! A Rust migration is identified by a `fn up(...)` declaration. If
//! `fn up` exists but no `fn down` / `fn rollback` exists in the same
//! file, the migration is one-way. Walks `function_item` nodes via
//! the AST so identifiers like `setup` or `lookup_user` don't trigger.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// `(has_up, has_down)` accumulated across the visit.
type State = (bool, bool);

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["function_item"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new((false, false)))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let st = state.unwrap().downcast_mut::<State>().unwrap();
        let source = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            return;
        };
        if name == "up" {
            st.0 = true;
        } else if name == "down" || name == "rollback" {
            st.1 = true;
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return;
        }
        let st = state.unwrap().downcast::<State>().unwrap();
        let (has_up, has_down) = *st;
        if has_up && !has_down {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: "migration-needs-rollback".into(),
                message: "Migration has `up()` but no `down()` / rollback — every migration must be reversible for quick recovery from bad deploys.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_path(src, &Check, "/app/migrations/001.rs")
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

    #[test]
    fn skips_non_migration_path() {
        let src = "fn up() { println!(\"create table\"); }";
        let diags = crate::rules::test_helpers::run_rust(src, &Check);
        assert!(diags.is_empty());
    }
}
