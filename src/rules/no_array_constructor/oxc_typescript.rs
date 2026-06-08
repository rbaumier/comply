//! no-array-constructor oxc backend — flag `new Array()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Array"])
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
        let Expression::Identifier(ident) = &new_expr.callee else {
            return;
        };
        if ident.name.as_str() != "Array" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid `new Array()` — use array literals `[]` instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_new_array_numeric() {
        assert_eq!(run_on("const a = new Array(3);").len(), 1);
    }


    #[test]
    fn flags_new_array_with_elements() {
        assert_eq!(run_on("const a = new Array(1, 2, 3);").len(), 1);
    }


    #[test]
    fn allows_array_literal() {
        assert!(run_on("const a = [1, 2, 3];").is_empty());
    }


    #[test]
    fn allows_array_from() {
        assert!(run_on("const a = Array.from({ length: 3 });").is_empty());
    }


    #[test]
    fn allows_new_map() {
        assert!(run_on("const m = new Map();").is_empty());
    }
}
