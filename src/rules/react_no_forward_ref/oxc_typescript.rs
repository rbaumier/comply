//! react-no-forward-ref oxc backend — flag `forwardRef(...)` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True if `expr` resolves to `forwardRef` — accepts both
/// `forwardRef(...)` (named import) and `React.forwardRef(...)` /
/// `*.forwardRef(...)` (namespace import).
fn callee_is_forward_ref(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == "forwardRef",
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "forwardRef"
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["forwardRef"])
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
        if !callee_is_forward_ref(&call.callee) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`forwardRef(...)` is deprecated in React 19 — accept `ref` \
                      as a regular prop on the component."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_named_forward_ref_call() {
        let src = r#"
            import { forwardRef } from "react";
            const Btn = forwardRef((props, ref) => <button ref={ref} />);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_namespaced_forward_ref_call() {
        let src = r#"
            import * as React from "react";
            const Btn = React.forwardRef((props, ref) => <button ref={ref} />);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_unrelated_calls() {
        let src = r#"const x = doStuff();"#;
        assert!(run(src).is_empty());
    }
}
