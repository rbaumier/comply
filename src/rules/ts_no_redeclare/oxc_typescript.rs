//! ts-no-redeclare OXC backend — detect duplicate variable declarations
//! via oxc_semantic symbol model.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let mut iter = scoping.symbol_declarations(symbol_id);
            if iter.next().is_none() {
                continue;
            }
            let name = scoping.symbol_name(symbol_id);
            for decl_id in iter {
                let span = nodes.kind(decl_id).span();
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "ts-no-redeclare".into(),
                    message: format!("`{name}` is already defined."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
