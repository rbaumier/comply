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
        if member.property.name.as_str() != "union" {
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
            message: "`.union()` deduplicates rows — use `.unionAll()` when \
                      rows are already unique."
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
    fn flags_drizzle_union() {
        assert_eq!(
            run_on("const q = db.select().from(a).union(db.select().from(b));").len(),
            1
        );
    }

    #[test]
    fn allows_union_all() {
        assert!(run_on("const q = db.select().from(a).unionAll(db.select().from(b));").is_empty());
    }

    #[test]
    fn does_not_flag_union_outside_query() {
        assert!(run_on("set.union(otherSet);").is_empty());
    }
}
