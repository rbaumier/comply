//! error-without-cause — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const ERROR_CTORS: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
];

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "EvalError",
            "URIError",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Constructor must be one of the built-in Error types.
        let ctor_name = match &new_expr.callee.without_parentheses() {
            Expression::Identifier(id) => &*id.name,
            _ => return,
        };
        if !ERROR_CTORS.contains(&ctor_name) {
            return;
        }

        // Arguments must contain a `.message` member access (the wrap signal)
        // and must NOT contain a `cause` field anywhere in args.
        let source = semantic.source_text();
        let args_span = new_expr.span();
        let args_text = &source[args_span.start as usize..args_span.end as usize];
        let wraps_existing = args_text.contains(".message");
        if !wraps_existing {
            return;
        }
        if args_text.contains("cause:") || args_text.contains("cause :") {
            return;
        }

        let (line, col) = byte_offset_to_line_col(source, new_expr.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "`new {ctor_name}(...)` wraps an existing error but drops `cause`. \
                 Add `{{ cause: original }}` as the 2nd argument to preserve the \
                 stack trace and cause chain — debuggers and `error.cause` rely on it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
