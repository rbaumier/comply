use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "select" {
            return;
        }
        if !call.arguments.is_empty() {
            return;
        }
        let after_src = &ctx.source[call.span.end as usize..];
        if !after_src.trim_start().starts_with(".from(") && !after_src.trim_start().starts_with(".from (") {
            let trimmed = after_src.trim_start();
            if !trimmed.starts_with('.') || !trimmed[1..].trim_start().starts_with("from") {
                return;
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`db.select()` without columns selects all fields — \
                      list columns explicitly with \
                      `db.select({ id: table.id, name: table.name })`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_empty_select_with_from() {
        assert_eq!(run_on("await db.select().from(users);").len(), 1);
    }

    #[test]
    fn allows_explicit_columns() {
        assert!(run_on("await db.select({ id: users.id }).from(users);").is_empty());
    }

    #[test]
    fn does_not_flag_select_outside_query() {
        assert!(run_on("arr.select();").is_empty());
    }
}
