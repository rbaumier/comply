use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportDefaultDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportDefaultDeclaration(decl) = node.kind() else {
            return;
        };

        if crate::rules::path_utils::is_config_file(ctx.path) {
            return;
        }
        if crate::rules::path_utils::is_framework_entry_point(ctx.path, ctx.project) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Default exports are forbidden \u{2014} use a named export instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
