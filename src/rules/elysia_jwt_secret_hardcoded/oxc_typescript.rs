//! elysia-jwt-secret-hardcoded oxc backend — flag hardcoded JWT secret literals.

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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jwt"])
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
        // Find `secret:` and look at the following token.
        let Some(off) = args_text.find("secret:") else { return };
        let after = args_text[off + "secret:".len()..].trim_start();

        // Hardcoded: starts with quote and is not env access.
        let starts_with_string = after.starts_with('\'') || after.starts_with('"') || after.starts_with('`');
        if !starts_with_string {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "JWT secret is a hardcoded string literal \u{2014} load from `process.env` instead.".into(),
            severity: Severity::Error,
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
    fn flags_hardcoded_secret() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: 'my-super-secret' }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_env_secret() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: process.env.JWT_SECRET! }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "jwt({ secret: 'literal' });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
