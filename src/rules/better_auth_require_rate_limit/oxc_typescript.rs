//! better-auth-require-rate-limit oxc backend — flag `betterAuth({ ... })` /
//! `createAuth({ ... })` whose config object lacks `rateLimit`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const AUTH_FACTORIES: &[&str] = &["betterAuth", "createAuth"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["rateLimit"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::Identifier(ident) = &call.callee else { return };
        if !AUTH_FACTORIES.contains(&ident.name.as_str()) {
            return;
        }

        // Find first object argument.
        let has_obj_with_rate_limit = call.arguments.iter().any(|arg| {
            let Some(expr) = arg.as_expression() else { return false };
            let Expression::ObjectExpression(obj) = expr else { return false };
            let obj_text = &ctx.source[obj.span.start as usize..obj.span.end as usize];
            obj_text.contains("rateLimit")
        });
        if has_obj_with_rate_limit {
            return;
        }

        // Check there is at least one object argument (otherwise no config to flag).
        let has_obj = call.arguments.iter().any(|arg| {
            let Some(expr) = arg.as_expression() else { return false };
            matches!(expr, Expression::ObjectExpression(_))
        });
        if !has_obj {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Better Auth config is missing `rateLimit` \u{2014} add `rateLimit: { enabled: true }` to protect auth endpoints.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_missing_rate_limit() {
        assert_eq!(
            run("export const auth = betterAuth({ database: db })").len(),
            1
        );
    }


    #[test]
    fn flags_missing_rate_limit_on_create_auth() {
        assert_eq!(run("createAuth({ database: db })").len(), 1);
    }


    #[test]
    fn allows_with_rate_limit() {
        assert!(run("export const auth = betterAuth({ rateLimit: { enabled: true } })").is_empty());
    }


    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = doSomething()").is_empty());
    }
}
