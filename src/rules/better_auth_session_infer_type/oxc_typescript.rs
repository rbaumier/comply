//! better-auth-session-infer-type OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn file_imports_better_auth(source: &str) -> bool {
    source.contains("from \"better-auth") || source.contains("from 'better-auth")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TSInterfaceDeclaration,
            AstType::TSTypeAliasDeclaration,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-auth"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (name, span_start, node_text) = match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) => {
                let start = decl.span.start as usize;
                let end = decl.span.end as usize;
                (
                    decl.id.name.as_str(),
                    start,
                    &ctx.source[start..end.min(ctx.source.len())],
                )
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                let start = decl.span.start as usize;
                let end = decl.span.end as usize;
                (
                    decl.id.name.as_str(),
                    start,
                    &ctx.source[start..end.min(ctx.source.len())],
                )
            }
            _ => return,
        };

        if name != "Session" {
            return;
        }

        if !file_imports_better_auth(ctx.source) {
            return;
        }

        // If the declaration already uses $Infer, skip.
        if node_text.contains("$Infer") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "Manual `Session` declaration — use `type Session = typeof auth.$Infer.Session` instead."
                    .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
