//! migration-needs-rollback — TS / JS / TSX backend.
//!
//! A migration file is identified by the presence of an `up` function:
//! `function up`, `async function up`, `up(...)` method on an object /
//! class, or `exports.up = ...`. If `up` exists but no `down` /
//! `rollback` function exists in the same file, the migration is
//! one-way and cannot be reverted.
//!
//! Walking the AST for function/method names — instead of substring
//! scanning — keeps the rule from firing on identifiers that contain
//! `up` (`setup`, `lookup`, `update_user`, …) or strings mentioning
//! "up" in prose.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

const FN_NODE_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "method_definition",
    "arrow_function",
    "pair",
    "assignment_expression",
    "public_field_definition",
];

/// `(has_up, has_down)` accumulated across the visit.
type State = (bool, bool);

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(FN_NODE_KINDS)
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
        let Some(name) = function_like_name(&node, source) else {
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
                path: std::sync::Arc::clone(&ctx.path_arc),
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

/// Extract the declared name of a function-like node, if any. Handles
/// the common shapes that TS/JS migration files use to declare `up` /
/// `down`:
/// - `function up() {}` → function_declaration with `name` field
/// - `{ up() {} }` / `class { up() {} }` → method_definition with `name`
/// - `{ up: () => {} }` → pair whose key is "up"
/// - `exports.up = () => {}` → assignment_expression to `exports.up`
fn function_like_name<'a>(node: &tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "function_declaration" | "method_definition" | "function" | "function_expression" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok()),
        "pair" | "public_field_definition" => node
            .child_by_field_name("key")
            .and_then(|n| n.utf8_text(source).ok()),
        "assignment_expression" => {
            let lhs = node.child_by_field_name("left")?;
            // `exports.up` / `module.exports.up` → use the property name.
            if lhs.kind() == "member_expression" {
                let prop = lhs.child_by_field_name("property")?;
                return prop.utf8_text(source).ok();
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, "/app/migrations/001.ts")
    }

    #[test]
    fn flags_up_without_down() {
        let src = "export async function up(db) { db.exec('CREATE TABLE t (id INT)'); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_up_with_down() {
        let src = "export async function up(db) {} export async function down(db) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_up_with_rollback() {
        let src = "export async function up(db) {} export async function rollback(db) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_migration() {
        let src = "function doStuff() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_setup_and_lookup() {
        // `setup` / `lookup` contain "up" but are not migration entry
        // points — substring matching used to flag these.
        let src = "function setup() {} function lookup() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_up_method_on_object() {
        let src = "module.exports = { async up(db) { return 1; } };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_up_down_methods_on_object() {
        let src = "module.exports = { async up(db) {}, async down(db) {} };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_exports_up_assignment() {
        let src = "exports.up = async function (db) {};";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_exports_up_and_down_assignment() {
        let src = "exports.up = async () => {}; exports.down = async () => {};";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_migration_path() {
        let src = "export async function up(db) { db.exec('CREATE TABLE t (id INT)'); }";
        let diags = crate::rules::test_helpers::run_ts(src, &Check);
        assert!(diags.is_empty());
    }
}
