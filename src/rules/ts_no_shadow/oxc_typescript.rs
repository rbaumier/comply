//! ts-no-shadow OXC backend — variable shadowing detection via oxc_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let scope_id = scoping.symbol_scope_id(symbol_id);
            let Some(parent_scope) = scoping.scope_parent_id(scope_id) else {
                continue;
            };
            let name = scoping.symbol_name(symbol_id);
            let ident = oxc_str::Ident::from(name);
            if scoping.find_binding(parent_scope, ident).is_some() {
                let span = scoping.symbol_span(symbol_id);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("`{name}` is already declared in an outer scope."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
