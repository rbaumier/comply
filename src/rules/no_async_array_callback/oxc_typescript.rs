//! no-async-array-callback OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const FLAGGED_METHODS: &[&str] = &[
    "forEach",
    "filter",
    "some",
    "every",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "forEach",
            "filter",
            "some",
            "every",
            "find",
            "findIndex",
            "findLast",
            "findLastIndex",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be obj.method(...)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !FLAGGED_METHODS.contains(&method) {
            return;
        }
        // First argument must be async.
        let Some(first) = call.arguments.first() else { return };
        let is_async = match first {
            oxc_ast::ast::Argument::ArrowFunctionExpression(f) => f.r#async,
            oxc_ast::ast::Argument::FunctionExpression(f) => f.r#async,
            _ => false,
        };
        if !is_async {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-async-array-callback".into(),
            message: format!(
                "`.{method}` does not await its callback \u{2014} the async work runs \
                 unsupervised. Use a `for...of` loop with `await`, or \
                 `Promise.all(arr.map(async ...))` for parallel awaited work."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
