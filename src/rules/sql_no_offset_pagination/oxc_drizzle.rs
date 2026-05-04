use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
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
        if member.property.name.as_str() != "offset" {
            return;
        }
        let receiver_src = &ctx.source[member.object.span().start as usize..member.object.span().end as usize];
        if !receiver_src.contains(".from(") && !receiver_src.contains(".select(") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "OFFSET pagination is O(N) — use cursor-based \
                      pagination with `.where(gt(col, cursor)).limit(N)` \
                      instead."
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
    fn flags_drizzle_offset_pagination() {
        assert_eq!(run_on("await db.select().from(users).offset(20).limit(10);").len(), 1);
    }

    #[test]
    fn allows_cursor_pagination() {
        assert!(run_on("await db.select().from(users).where(gt(users.id, cursor)).limit(10);").is_empty());
    }

    #[test]
    fn does_not_flag_offset_outside_query() {
        assert!(run_on("arr.offset(5);").is_empty());
    }
}
