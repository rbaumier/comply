use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LabeledStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LabeledStatement(labeled) = node.kind() else {
            return;
        };

        // Check if any ancestor is a SwitchStatement.
        let inside_switch = semantic
            .nodes()
            .ancestors(node.id())
            .any(|a| matches!(a.kind(), AstKind::SwitchStatement(_)));

        if !inside_switch {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, labeled.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Label inside switch statement \u{2014} this is a JS label, not a case branch. Use `case <value>:` instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
