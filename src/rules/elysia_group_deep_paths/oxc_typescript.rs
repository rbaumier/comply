//! OxcCheck backend for elysia-group-deep-paths.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

fn segment_count(path: &str) -> usize {
    path.split('/').filter(|s| !s.is_empty()).count()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&method) {
            return;
        }

        // First argument should be a string path.
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(Expression::StringLiteral(path_lit)) = first_arg.as_expression() else { return };
        let unquoted = path_lit.value.as_str();
        if segment_count(unquoted) < 3 {
            return;
        }

        // Skip if inside a `.group()` call.
        let nodes = semantic.nodes();
        let mut current = node.id();
        loop {
            let parent_id = nodes.parent_id(current);
            if parent_id == current {
                break;
            }
            let parent = nodes.get_node(parent_id);
            if let AstKind::CallExpression(parent_call) = parent.kind() {
                if let Expression::StaticMemberExpression(pm) = &parent_call.callee {
                    if pm.property.name.as_str() == "group" {
                        return;
                    }
                }
            }
            current = parent_id;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Path `{unquoted}` has {} segments — consider grouping with `.group()` or a `prefix`.",
                segment_count(unquoted)
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
