//! OxcCheck backend for elysia-apollo-playground-prod.

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

        // callee must be `apollo`
        let Expression::Identifier(callee_id) = &call.callee else { return };
        if callee_id.name.as_str() != "apollo" {
            return;
        }

        // Check args text for `enablePlayground: true` (whitespace-insensitive).
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if !norm.contains("enablePlayground:true") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`apollo({ enablePlayground: true })` is unconditional — gate it on a non-production env flag.".into(),
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
    fn flags_enable_playground_true() {
        let src = "import { apollo } from '@elysiajs/apollo';\napp.use(apollo({ enablePlayground: true }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_env_gated_playground() {
        let src = "import { apollo } from '@elysiajs/apollo';\napp.use(apollo({ enablePlayground: process.env.NODE_ENV !== 'production' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_apollo_files() {
        let src = "server({ enablePlayground: true });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
