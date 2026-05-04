//! elysia-custom-errors-in-model OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
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

        let path_str = ctx.path.to_string_lossy();
        if !path_str.contains("service") {
            return;
        }

        let AstKind::Class(class) = node.kind() else { return };

        // Check for `extends Error`.
        let Some(super_class) = &class.super_class else { return };
        let is_error = match super_class {
            Expression::Identifier(id) => id.name == "Error",
            _ => false,
        };
        if !is_error { return; }

        let (line, column) = byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Custom error class belongs in the matching `*.model.ts` so `.error({ ... })` mapping stays co-located with the schema.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
