//! OXC backend for no-useless-switch-case.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
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
        if cases.len() < 2 {
            return;
        }

        // Last case must be `default` (test is None).
        let last = &cases[cases.len() - 1];
        if last.test.is_some() {
            return;
        }

        // Walk backwards from the case just before default and flag empty cases.
        let mut i = cases.len() - 2;
        loop {
            let case = &cases[i];
            // Must be a `case X:` (not default).
            if case.test.is_none() {
                break;
            }

            // A case is "empty" if it has no consequent statements
            // (or only empty statements / comments aren't represented in OXC AST).
            let is_empty = case.consequent.is_empty()
                || case.consequent.iter().all(|s| matches!(s, Statement::EmptyStatement(_)));

            if !is_empty {
                break;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, case.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Useless case in switch statement — it falls through \
                          to `default` with no own code."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });

            if i == 0 {
                break;
            }
            i -= 1;
        }
    }
}
