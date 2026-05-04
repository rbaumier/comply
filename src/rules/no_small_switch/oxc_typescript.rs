//! OxcCheck backend for no-small-switch — flag `switch` with fewer than N `case` clauses.

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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchStatement(switch) = node.kind() else { return };
        // Count only non-default cases (cases with a test expression).
        let case_count = switch.cases.iter().filter(|c| c.test.is_some()).count();
        let min_cases = ctx.config.threshold("no-small-switch", "min_cases", ctx.lang);
        if case_count < min_cases {
            let (line, column) = byte_offset_to_line_col(ctx.source, switch.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`switch` has only {case_count} case(s) — use `if/else` instead."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
