use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if matches!(ancestor.kind(), AstKind::SwitchStatement(_)) {
                let AstKind::SwitchStatement(switch) = node.kind() else { return };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, switch.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-nested-switch".into(),
                    message: "Nested `switch` — extract the inner switch into a separate function."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }
    }
}
