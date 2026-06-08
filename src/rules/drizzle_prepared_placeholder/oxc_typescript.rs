//! OxcCheck backend — flag `.prepare()` chains with `.where()` but no `placeholder()`.

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
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be `*.prepare`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "prepare" {
            return;
        }
        // The call span covers the entire chain (e.g. `db.select()...prepare('q')`).
        let chain = &ctx.source[call.span.start as usize..call.span.end as usize];
        if !chain.contains(".where(") {
            return;
        }
        if chain.contains("placeholder(") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.prepare()` with `.where(...)` must use `sql.placeholder('name')` instead of inline variables so the prepared statement can be reused across executions.".into(),
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
    fn flags_prepare_with_inline_where() {
        let src = "const q = db.select().from(u).where(eq(u.id, id)).prepare('q')";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_prepare_with_placeholder() {
        let src =
            "const q = db.select().from(u).where(eq(u.id, sql.placeholder('id'))).prepare('q')";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_prepare_without_where() {
        let src = "const q = db.select().from(u).prepare('q')";
        assert!(run(src).is_empty());
    }
}
