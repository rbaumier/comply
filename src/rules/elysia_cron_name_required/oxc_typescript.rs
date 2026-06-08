//! OxcCheck backend for elysia-cron-name-required.

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // callee must be `cron`
        let Expression::Identifier(callee_id) = &call.callee else { return };
        if callee_id.name.as_str() != "cron" {
            return;
        }

        // Check args text for `name:` (whitespace-insensitive).
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("name:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`cron({ ... })` is missing `name:` — required for stop()/diagnostics.".into(),
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
    fn flags_cron_without_name() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ pattern: '* * * * *', run() {} }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_cron_with_name() {
        let src = "import { cron } from '@elysiajs/cron';\napp.use(cron({ name: 'cleanup', pattern: '* * * * *', run() {} }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_cron_files() {
        let src = "cron({ pattern: '* * * * *' });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
