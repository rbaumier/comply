//! drizzle-created-at-default-now — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Walk a call-expression chain to find the root identifier name.
/// E.g. `timestamp('created_at').defaultNow().notNull()` → `"timestamp"`.
fn base_call_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::CallExpression(call) => match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            Expression::StaticMemberExpression(member) => base_call_name(&member.object),
            _ => None,
        },
        _ => None,
    }
}

/// Get the full text of an expression from source.
fn expr_text<'a, 'b>(expr: &Expression<'b>, source: &'a str) -> &'a str {
    let start = expr.span().start as usize;
    let end = expr.span().end as usize;
    source.get(start..end).unwrap_or("")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "createdAt" && key_name != "created_at" {
            return;
        }

        // Value must be a call expression whose base is `timestamp`.
        let Expression::CallExpression(_) = &prop.value else { return };
        let Some(ctor) = base_call_name(&prop.value) else { return };
        if ctor != "timestamp" {
            return;
        }

        let chain_text = expr_text(&prop.value, ctx.source);
        if chain_text.contains(".defaultNow(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`createdAt` timestamp column must chain `.defaultNow()` so inserts auto-populate the value.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_created_at_without_default_now() {
        let src = "const t = { createdAt: timestamp('created_at').notNull() }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_created_at_with_default_now() {
        let src = "const t = { createdAt: timestamp('created_at').defaultNow().notNull() }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_non_timestamp() {
        let src = "const t = { createdAt: text('created_at') }";
        assert!(run(src).is_empty());
    }
}
