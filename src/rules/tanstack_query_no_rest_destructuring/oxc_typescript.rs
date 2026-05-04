//! tanstack-query-no-rest-destructuring oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "useMutation",
    "useQueries",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        // Check that the binding is an object pattern with a rest element.
        let BindingPattern::ObjectPattern(obj_pat) = &decl.id else {
            return;
        };
        if obj_pat.rest.is_none() {
            return;
        }

        // Check that the initializer is a call to a query hook.
        let Some(init) = &decl.init else {
            return;
        };
        let Expression::CallExpression(call) = init else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        if !QUERY_HOOKS.contains(&name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, obj_pat.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Rest destructuring on `{name}()` result — destructure only the fields you actually use."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
