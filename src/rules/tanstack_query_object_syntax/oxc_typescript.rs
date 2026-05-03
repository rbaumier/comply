//! tanstack-query-object-syntax oxc backend — flag positional hook calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const HOOKS: &[&str] = &[
    "useQuery",
    "useMutation",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
];

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let func_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !HOOKS.contains(&func_name) {
            return;
        }

        let Some(first) = call.arguments.first() else { return };

        // Allow object, identifier, and call_expression (factory) as first arg.
        match first {
            Argument::ObjectExpression(_) => return,
            Argument::Identifier(_) => return,
            Argument::CallExpression(_) => return,
            _ => {}
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{func_name}` must be called with an options object: \
                 `{func_name}({{ queryKey, queryFn }})`. The positional form was removed in v5."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
