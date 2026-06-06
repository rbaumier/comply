//! react-no-cookies-in-layout OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const DYNAMIC_FNS: &[&str] = &["cookies", "headers"];

/// Returns `true` if `source` contains an import from `next/headers`.
fn has_next_headers_import(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from 'next/headers'") || crate::oxc_helpers::source_contains(source, "from \"next/headers\"")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["next/headers"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Only fire on files named `layout.*`.
        let file_stem = ctx.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if file_stem != "layout" {
            return;
        }

        if !has_next_headers_import(ctx.source) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let callee_text = callee.name.as_str();

        if DYNAMIC_FNS.contains(&callee_text) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{callee_text}()` in a layout file forces EVERY child page to \
                     be dynamically rendered. Move it to the individual page \
                     that needs it."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
