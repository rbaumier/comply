//! tanstack-query-no-is-loading oxc backend.
//!
//! Flag `isLoading` identifiers in files that also call a TanStack Query hook.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const HOOKS: &[&str] = &[
    "useQuery",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "useQueries",
];

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["isLoading"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // First pass: check if any call expression calls a query hook.
        let mut has_hook = false;
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind() {
                if let Expression::Identifier(ident) = &call.callee {
                    if HOOKS.contains(&ident.name.as_str()) {
                        has_hook = true;
                        break;
                    }
                }
            }
        }
        if !has_hook {
            return Vec::new();
        }

        // Second pass: flag all `isLoading` identifiers.
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let name = match node.kind() {
                AstKind::IdentifierReference(ident) => ident.name.as_str(),
                AstKind::BindingIdentifier(ident) => ident.name.as_str(),
                _ => continue,
            };
            if name != "isLoading" {
                continue;
            }
            let span = match node.kind() {
                AstKind::IdentifierReference(ident) => ident.span,
                AstKind::BindingIdentifier(ident) => ident.span,
                _ => continue,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`isLoading` was removed in TanStack Query v5 — use `isPending` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
