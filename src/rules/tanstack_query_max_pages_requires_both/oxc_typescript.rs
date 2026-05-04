//! tanstack-query-max-pages-requires-both OXC backend — flag
//! `useInfiniteQuery({ maxPages: N })` missing either `getNextPageParam`
//! or `getPreviousPageParam`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

const INFINITE_CALLS: &[&str] = &[
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "infiniteQueryOptions",
];

pub struct Check;

fn has_key(props: &oxc_allocator::Vec<'_, ObjectPropertyKind<'_>>, needle: &str) -> bool {
    for prop in props {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name == needle {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["maxPages"])
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

        // Check callee is one of the infinite query functions
        let func_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !INFINITE_CALLS.contains(&func_name) {
            return;
        }

        // First argument must be an object expression
        let Some(first) = call.arguments.first() else {
            return;
        };
        let oxc_ast::ast::Argument::ObjectExpression(obj) = first else {
            return;
        };

        if !has_key(&obj.properties, "maxPages") {
            return;
        }

        let has_next = has_key(&obj.properties, "getNextPageParam");
        let has_prev = has_key(&obj.properties, "getPreviousPageParam");
        if has_next && has_prev {
            return;
        }

        let missing = match (has_next, has_prev) {
            (false, false) => "`getNextPageParam` and `getPreviousPageParam`",
            (false, true) => "`getNextPageParam`",
            (true, false) => "`getPreviousPageParam`",
            _ => unreachable!(),
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`maxPages` is set on `{func_name}` but {missing} is missing. Both page-param functions are required."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
