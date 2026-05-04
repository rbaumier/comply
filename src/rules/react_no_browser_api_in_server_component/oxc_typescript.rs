//! react-no-browser-api-in-server-component OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const BROWSER_GLOBALS: &[&str] = &[
    "window",
    "document",
    "localStorage",
    "sessionStorage",
    "navigator",
    "location",
];

pub struct Check;

fn is_inside_typeof(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::UnaryExpression(unary) = parent.kind() {
            if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                return true;
            }
        }
        current_id = parent_id;
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.rsc_context != RscContext::ServerComponent {
            return;
        }

        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        let Expression::Identifier(ident) = &member.object else {
            return;
        };
        let name = ident.name.as_str();
        if !BROWSER_GLOBALS.contains(&name) {
            return;
        }

        if is_inside_typeof(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-no-browser-api-in-server-component".into(),
            message: format!(
                "`{name}` is a browser global and doesn't exist on the server. \
                 Gate this behind `\"use client\"` or a client-only boundary."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
