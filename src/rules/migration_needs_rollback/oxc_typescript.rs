//! migration-needs-rollback OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return Vec::new();
        }

        let nodes = semantic.nodes();
        let mut has_up = false;
        let mut has_down = false;

        for node in nodes.iter() {
            let name = match node.kind() {
                AstKind::Function(func) => {
                    func.id.as_ref().map(|id| id.name.as_str())
                }
                AstKind::ArrowFunctionExpression(_) => {
                    // Check if this arrow is assigned: `exports.up = () => {}`
                    // or `{ up: () => {} }` — handled via parent
                    None
                }
                AstKind::ObjectProperty(prop) => {
                    // `{ up() {} }` or `{ up: () => {} }`
                    match &prop.key {
                        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
                        _ => None,
                    }
                }
                AstKind::AssignmentExpression(assign) => {
                    // `exports.up = ...`
                    if let AssignmentTarget::StaticMemberExpression(member) = &assign.left {
                        Some(member.property.name.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(n) = name {
                if n == "up" {
                    has_up = true;
                } else if n == "down" || n == "rollback" {
                    has_down = true;
                }
            }
        }

        if has_up && !has_down {
            vec![Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Migration has `up()` but no `down()` / rollback \
                          — every migration must be reversible for quick \
                          recovery from bad deploys."
                    .into(),
                severity: Severity::Warning,
                span: None,
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "/app/migrations/001.ts")
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
        let diags = crate::rules::test_helpers::run_oxc_ts(src, &Check);
        assert!(diags.is_empty());
    }
}
