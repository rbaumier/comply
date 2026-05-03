//! elysia-html-import-uppercase oxc backend — flag missing `Html` JSX factory import.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@elysiajs/html"])
    }

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

        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        if import.source.value.as_str() != "@elysiajs/html" {
            return;
        }

        // Check if any named import specifier imports `Html` (the local name
        // before any `as` alias).
        let imports_html = import
            .specifiers
            .as_ref()
            .map(|specs| {
                specs.iter().any(|s| {
                    if let ImportDeclarationSpecifier::ImportSpecifier(named) = s {
                        // The imported name (before `as`)
                        named.imported.name().as_str() == "Html"
                    } else {
                        false
                    }
                })
            })
            .unwrap_or(false);

        if imports_html {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Import `Html` (uppercase) from `@elysiajs/html` — JSX needs the factory binding to be in scope.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
