//! OxcCheck backend for ts-no-promise-void-function-misuse.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const DIRECT_CALLEES: &[&str] = &[
    "setTimeout",
    "setInterval",
    "setImmediate",
    "queueMicrotask",
];

const MEMBER_METHODS: &[&str] = &[
    "forEach",
    "map",
    "filter",
    "reduce",
    "some",
    "every",
    "find",
    "findIndex",
    "nextTick",
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let (matches, display) = match &call.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                (DIRECT_CALLEES.contains(&name), name.to_string())
            }
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if MEMBER_METHODS.contains(&prop) {
                    let obj_text =
                        &ctx.source[member.object.span().start as usize..member.object.span().end as usize];
                    (true, format!("{obj_text}.{prop}"))
                } else {
                    (false, String::new())
                }
            }
            _ => (false, String::new()),
        };

        if !matches {
            return;
        }

        // Check the first argument for async
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        if !is_async_arg(first_arg) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{display}(async ...)` ignores the returned promise. Wrap with \
                 `() => {{ void asyncFn(); }}` or refactor `.forEach` into a `for ... of` with `await`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_async_arg(arg: &Argument) -> bool {
    match arg {
        Argument::ArrowFunctionExpression(arrow) => arrow.r#async,
        Argument::FunctionExpression(func) => func.r#async,
        _ => false,
    }
}
