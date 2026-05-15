//! ts-no-explicit-any oxc backend — flag TSAnyKeyword.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAnyKeyword]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAnyKeyword(kw) = node.kind() else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, kw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Explicit `any` disables type checking — prefer `unknown` (forces \
                      narrowing at the use site) or a precise type."
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
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_param_typed_any() {
        let src = "function f(x: any): number { return 0; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_as_any_cast() {
        let src = "const x = something as any;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_unknown() {
        let src = "function f(x: unknown): number { return 0; }";
        assert!(run(src).is_empty());
    }
}
