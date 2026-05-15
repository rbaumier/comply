//! promise-no-new-statics oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const PROMISE_STATICS: &[&str] =
    &["resolve", "reject", "all", "allSettled", "race", "any"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new Promise."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &new_expr.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Promise" {
            return;
        }
        let static_name = member.property.name.as_str();
        if !PROMISE_STATICS.contains(&static_name) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`new Promise.{static_name}(...)` calls a static as a constructor — \
                 drop the `new` and call `Promise.{static_name}(...)` directly."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_new_promise_resolve() {
        let src = r#"const p = new Promise.resolve(1);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_new_promise_all() {
        let src = r#"const p = new Promise.all([]);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_resolve() {
        let src = r#"const p = Promise.resolve(1);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_promise_executor() {
        let src = r#"const p = new Promise((resolve) => resolve(1));"#;
        assert!(run(src).is_empty());
    }
}
