//! react-no-find-dom-node oxc backend.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let (matched, span_start) = match &call.callee {
            Expression::StaticMemberExpression(member) => {
                let is_find = member.property.name.as_str() == "findDOMNode";
                (is_find, member.span.start)
            }
            Expression::Identifier(ident) => {
                let is_find = ident.name.as_str() == "findDOMNode";
                (is_find, ident.span.start)
            }
            _ => (false, 0),
        };

        if !matched {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`findDOMNode` is deprecated in React 19 — use refs instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
