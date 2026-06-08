//! elysia-jwt-missing-exp oxc backend — flag jwt config without `exp`.

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
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "jwt" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        // Only flag if there is at least one option object (skip `jwt()` with no args).
        let inner = args_text.trim();
        if inner.ends_with("()") && inner.starts_with("jwt(") && inner.len() == 5 {
            return;
        }

        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("exp:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "JWT config is missing `exp` \u{2014} tokens will never expire.".into(),
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
    fn flags_missing_exp() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: process.env.S! }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_with_exp() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: process.env.S!, exp: '7d' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "jwt({ name: 'jwt' });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
