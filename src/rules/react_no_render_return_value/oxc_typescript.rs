//! react-no-render-return-value oxc backend — detect `ReactDOM.render(...)`
//! whose result is captured (assigned, returned, awaited, etc.).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["ReactDOM"])
    }

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is `ReactDOM.render`.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "ReactDOM" || member.property.name.as_str() != "render" {
            return;
        }

        // A standalone statement is fine: `ReactDOM.render(...)` as a line.
        if let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) {
            if matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not use the return value of `ReactDOM.render()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
