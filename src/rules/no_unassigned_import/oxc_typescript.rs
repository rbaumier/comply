//! no-unassigned-import oxc backend — flag side-effect imports.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Known CSS/style extensions that are legitimate side-effect imports.
const STYLE_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".sass", ".less", ".styl", ".stylus", ".pcss", ".postcss",
];

fn is_style_import(source: &str) -> bool {
    STYLE_EXTENSIONS.iter().any(|ext| source.ends_with(ext))
}

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
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        // A side-effect import has no specifiers.
        if import.specifiers.as_ref().is_some_and(|s| !s.is_empty()) {
            return;
        }

        let unquoted = import.source.value.as_str();

        if is_style_import(unquoted) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Side-effect import `{unquoted}` \u{2014} imported module should be assigned."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
