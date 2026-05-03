//! OxcCheck backend — flag `sql` tagged templates containing `IN (`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TaggedTemplateExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TaggedTemplateExpression(tagged) = node.kind() else { return };
        // Tag must be `sql`
        let Expression::Identifier(tag) = &tagged.tag else { return };
        if tag.name.as_str() != "sql" {
            return;
        }
        // Check quasis for `IN (` (case-insensitive)
        let has_in = tagged.quasi.quasis.iter().any(|q| {
            let upper = q.value.raw.to_ascii_uppercase();
            upper.contains(" IN (") || upper.contains("\nIN (") || upper.contains("\tIN (")
        });
        if !has_in {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, tagged.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`sql` template contains `IN (...)` \u{2014} prefer `inArray(col, [...])` for a parameterized, typed alternative.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
