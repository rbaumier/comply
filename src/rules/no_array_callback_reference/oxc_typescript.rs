//! no-array-callback-reference OXC backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ITERATOR_METHODS: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "reduce",
    "reduceRight",
    "some",
];

const IGNORED_IDENTIFIERS: &[&str] = &["Boolean", "String", "Number", "BigInt", "Symbol"];

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

        // Must be a member expression call: `something.method(callback)`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !ITERATOR_METHODS.contains(&method_name) {
            return;
        }

        // Get the first argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        match expr {
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if IGNORED_IDENTIFIERS.contains(&name) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass function `{name}` directly to `.{method_name}(…)` — use `(…) => {name}(…)` instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            Expression::StaticMemberExpression(inner_member) => {
                let text = &ctx.source
                    [inner_member.span.start as usize..inner_member.span.end as usize];
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, inner_member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass `{text}` directly to `.{method_name}(…)` — wrap it in an arrow function."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
