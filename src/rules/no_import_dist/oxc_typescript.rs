//! no-import-dist OXC backend — flag imports targeting `dist/` build output.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns true if `spec` points into a `dist/` directory.
fn targets_dist(spec: &str) -> bool {
    spec.contains("/dist/") || spec.starts_with("dist/")
}

fn emit(ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>, spec: &str, offset: usize) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Import from '{spec}' targets `dist/`. Import from package entry point, not dist/."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                let spec = import.source.value.as_str();
                if targets_dist(spec) {
                    emit(ctx, diagnostics, spec, import.span.start as usize);
                }
            }
            AstKind::CallExpression(call) => {
                // require('pkg/dist/foo')
                let is_require = matches!(
                    &call.callee,
                    oxc_ast::ast::Expression::Identifier(id) if id.name.as_str() == "require"
                );
                if !is_require {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let spec = match first_arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.value.as_str(),
                    _ => return,
                };
                if targets_dist(spec) {
                    emit(ctx, diagnostics, spec, call.span.start as usize);
                }
            }
            _ => {}
        }
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Handle dynamic import() which is ImportExpression, not CallExpression
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::ImportExpression(import) = node.kind() {
                if let oxc_ast::ast::Expression::StringLiteral(s) = &import.source {
                    let spec = s.value.as_str();
                    if targets_dist(spec) {
                        emit(ctx, &mut diagnostics, spec, import.span.start as usize);
                    }
                }
            }
        }
        diagnostics
    }
}
