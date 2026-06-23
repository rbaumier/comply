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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    #[test]
    fn flags_up_without_down_in_migration() {
        let src = "exports.up = function () {};";
        assert_eq!(run_on(src, "migrations/001-init.js").len(), 1);
    }

    #[test]
    fn allows_up_with_down_in_migration() {
        let src = "exports.up = function () {}; exports.down = function () {};";
        assert!(run_on(src, "migrations/001-init.js").is_empty());
    }

    /// Regression for #5786: a migration runner's own integration test
    /// defines `up` stubs and calls `dbmigrate.up()` without being a real
    /// migration. The repo dir name (`node-db-migrate`) makes every path a
    /// migration path, so the test-dir skip is what keeps it quiet.
    #[test]
    fn does_not_flag_up_stub_in_migration_runner_test() {
        let src = "function up() {} module.exports = { up };";
        assert!(
            run_on(src, "node-db-migrate/test/integration/api_test.js").is_empty(),
            "an `up` stub in a migration runner's test file must not be flagged"
        );
    }

    #[test]
    fn still_flags_real_migration_missing_down() {
        let src = "exports.up = function () {};";
        assert_eq!(
            run_on(src, "node-db-migrate/migrations/20240101-add-users.js").len(),
            1,
            "a real migration missing its `down` is still flagged"
        );
    }
}
