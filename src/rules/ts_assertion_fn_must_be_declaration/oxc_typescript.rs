//! OxcCheck backend for ts-assertion-fn-must-be-declaration — flag arrow functions with `asserts` return type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["asserts"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ArrowFunctionExpression(arrow) = node.kind() else { return };
        // Check if the return type annotation contains "asserts ".
        let Some(ref rt) = arrow.return_type else { return };
        let rt_start = rt.span.start as usize;
        let rt_end = rt.span.end as usize;
        let rt_text = &ctx.source[rt_start..rt_end];
        if !rt_text.contains("asserts ") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertion functions (`asserts`) must be declared with `function`, not as an arrow.".into(),
            severity: Severity::Error,
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
    fn flags_arrow_with_asserts_predicate() {
        let src = "const assertIsString = (x: unknown): asserts x is string => { if (typeof x !== 'string') throw 0; };";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_arrow_with_bare_asserts() {
        let src = "const check = (x: unknown): asserts x => { if (!x) throw 0; };";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_function_declaration_with_asserts() {
        let src = "function assertIsString(x: unknown): asserts x is string { if (typeof x !== 'string') throw 0; }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_regular_arrow() {
        let src = "const f = (x: number): string => String(x);";
        assert!(run(src).is_empty());
    }
}
