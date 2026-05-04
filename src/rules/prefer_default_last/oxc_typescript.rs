//! prefer-default-last OXC backend — flag `default` clause not last in switch.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::SwitchCase;
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
        let AstKind::SwitchStatement(switch) = node.kind() else {
            return;
        };

        let cases = &switch.cases;
        let mut default_idx: Option<usize> = None;
        let mut last_case_idx: Option<usize> = None;

        for (i, case) in cases.iter().enumerate() {
            if case.test.is_none() {
                default_idx = Some(i);
            } else {
                last_case_idx = Some(i);
            }
        }

        if let (Some(di), Some(lci)) = (default_idx, last_case_idx)
            && di < lci {
                let default_case: &SwitchCase = &cases[di];
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, default_case.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "`default` clause should be the last clause in the switch statement."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
    }
}
