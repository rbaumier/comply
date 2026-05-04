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
