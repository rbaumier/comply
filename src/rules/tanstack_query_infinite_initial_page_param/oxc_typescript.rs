use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const INFINITE_CALLS: &[&str] = &[
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "infiniteQueryOptions",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["initialPageParam"])
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
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        let func_name = ident.name.as_str();
        if !INFINITE_CALLS.contains(&func_name) {
            return;
        }
        let Some(first_arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
            return;
        };
        let Expression::ObjectExpression(obj) = first_arg else {
            return;
        };
        let has_initial_page_param = obj.properties.iter().any(|p| {
            let ObjectPropertyKind::ObjectProperty(prop) = p else {
                return false;
            };
            match &prop.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str() == "initialPageParam",
                PropertyKey::StringLiteral(s) => s.value.as_str() == "initialPageParam",
                _ => false,
            }
        });
        if has_initial_page_param {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{func_name}` is missing `initialPageParam`. Required in v5 — add e.g. `initialPageParam: 0`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
