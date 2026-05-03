//! elysia-eden-error-unchecked oxc backend — flag `{ data }` destructuring without `error`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };
        let oxc_ast::ast::BindingPattern::ObjectPattern(pattern) = &decl.id else {
            return;
        };
        // Check if it's exactly `{ data }` — one property named "data", no rest.
        if pattern.rest.is_some() || pattern.properties.len() != 1 {
            return;
        }
        let prop = &pattern.properties[0];
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &prop.value else {
            return;
        };
        if ident.name.as_str() != "data" {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, pattern.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Eden treaty returns `{ data, error }` — destructure both and check `error` before using `data`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
