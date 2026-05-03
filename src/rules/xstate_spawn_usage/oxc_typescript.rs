//! xstate-spawn-usage OXC backend.
//!
//! Flag `spawn(...)` calls not nested inside an `assign(...)` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["spawn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Must be bare `spawn(...)`.
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "spawn" {
            return;
        }

        // Must have xstate dependency.
        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else { return };
        if !pkg.has_dep_or_engine("xstate") {
            return;
        }

        // Walk ancestors; if any is an `assign(...)` call, we're fine.
        let nodes = semantic.nodes();
        let mut cur_id = nodes.parent_id(node.id());
        loop {
            if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
                break;
            }
            if let AstKind::CallExpression(ancestor_call) = nodes.kind(cur_id) {
                if let Expression::Identifier(id) = &ancestor_call.callee {
                    if id.name.as_str() == "assign" {
                        return;
                    }
                }
            }
            cur_id = nodes.parent_id(cur_id);
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`spawn()` must be called inside an `assign()` action.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
