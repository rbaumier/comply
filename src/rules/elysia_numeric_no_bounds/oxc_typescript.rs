//! OxcCheck backend — flag bare `t.Number()` / `t.Numeric()` calls (no bounds option).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["exclusiveMaximum", "exclusiveMinimum", "maximum", "minimum"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "t.Number" && callee_text != "t.Numeric" {
            return;
        }

        let args_start = call.callee.span().end as usize;
        let args_end = call.span.end as usize;
        let args_text = &ctx.source[args_start..args_end];

        if args_text.contains("minimum")
            || args_text.contains("maximum")
            || args_text.contains("exclusiveMinimum")
            || args_text.contains("exclusiveMaximum")
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`t.Number()` / `t.Numeric()` without `minimum`/`maximum` accepts any numeric value, including IDs <= 0.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_bare_number() {
        let src = "import { t } from 'elysia';\nconst s = t.Number();";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_bare_numeric() {
        let src = "import { t } from 'elysia';\nconst s = t.Numeric();";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_number_with_minimum() {
        let src = "import { t } from 'elysia';\nconst s = t.Number({ minimum: 1 });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.Number();";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
