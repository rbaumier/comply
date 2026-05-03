//! no-let oxc backend — flag `let` declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["let"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };
            if decl.kind != oxc_ast::ast::VariableDeclarationKind::Let {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`let` creates a mutable binding — use `const` instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
