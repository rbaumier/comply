//! elysia-service-coupled oxc backend — flag elysia imports inside service modules.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
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

        let path_str = ctx.path.to_string_lossy().to_lowercase();
        if !path_str.contains("service") {
            return;
        }

        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        let source_value = import.source.value.as_str();
        if source_value != "elysia" {
            return;
        }

        // Extract named specifiers.
        let Some(specifiers) = &import.specifiers else { return };
        let names: Vec<&str> = specifiers
            .iter()
            .filter_map(|s| {
                if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(spec) = s {
                    Some(spec.imported.name().as_str())
                } else {
                    None
                }
            })
            .collect();

        if names.is_empty() {
            return;
        }
        if names.iter().all(|n| *n == "status") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Service modules should not import framework symbols from `elysia` (only `status` is allowed). Move HTTP concerns to the route layer.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
