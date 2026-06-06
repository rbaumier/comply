//! react-prefer-use-transition oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

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
        // Skip if file already uses useTransition.
        if ctx.source_contains("useTransition") {
            return;
        }

        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        // Check initializer is `useState(false)`.
        let Some(init) = &decl.init else { return };
        let Expression::CallExpression(call) = init else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useState" {
            return;
        }
        // Check first argument is `false`.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(Expression::BooleanLiteral(lit)) = first_arg.as_expression() else {
            return;
        };
        if lit.value {
            return;
        }

        // Check binding is array pattern with 2 identifiers.
        let BindingPattern::ArrayPattern(arr) = &decl.id else {
            return;
        };
        if arr.elements.len() != 2 {
            return;
        }
        let Some(Some(second)) = arr.elements.get(1) else {
            return;
        };
        let BindingPattern::BindingIdentifier(setter_ident) = second else {
            return;
        };
        let setter = setter_ident.name.as_str();
        if setter.is_empty() {
            return;
        }

        // Check that the file calls `setter(true)`, `setter(false)`, and `await`.
        let src = ctx.source;
        if !src.contains(&format!("{setter}(true)"))
            || !src.contains(&format!("{setter}(false)"))
            || !src.contains("await ")
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, decl.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Replace manual `{setter}(true/false)` loading state with `useTransition`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
