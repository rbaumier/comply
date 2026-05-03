//! OXC backend for no-array-method-this-argument — flag the `thisArg`
//! parameter in array methods like `.filter(fn, context)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

const METHODS_WITH_THIS_ARG: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "some",
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

        // Must be a member expression call: `something.method(...)`.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let method_name = member.property.name.as_str();
        if !METHODS_WITH_THIS_ARG.contains(&method_name) {
            return;
        }

        // Check that there are exactly 2 arguments (callback + thisArg).
        if call.arguments.len() != 2 {
            return;
        }

        let this_arg = &call.arguments[1];
        let span = this_arg.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Do not use the `this` argument in `Array#{}()` — use `.bind()` or an arrow function instead.",
                method_name
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
