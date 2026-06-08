use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".listen"])
    }

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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_span = call.callee.span();
        let callee_text = &ctx.source[callee_span.start as usize..callee_span.end as usize];
        if !callee_text.ends_with(".listen") {
            return;
        }

        // First positional argument must be a numeric literal
        let Some(first_arg) = call.arguments.first() else { return };
        let arg_expr = first_arg.as_expression();
        let Some(expr) = arg_expr else { return };
        if !matches!(expr, Expression::NumericLiteral(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Hardcoded port in `.listen()` — read from `process.env.PORT` so deploy platforms can override it.".into(),
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
    fn flags_numeric_port() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_env_port() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().listen(process.env.PORT ?? 3000);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "server.listen(3000);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
