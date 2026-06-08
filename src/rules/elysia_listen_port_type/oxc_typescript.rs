//! elysia-listen-port-type — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process.env.PORT"])
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

        // Callee must be `.listen`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "listen" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let arg_start = first_arg.span().start as usize;
        let arg_end = first_arg.span().end as usize;
        let first_text = ctx.source.get(arg_start..arg_end).unwrap_or("");
        if !first_text.contains("process.env.PORT") {
            return;
        }
        if first_text.contains("Number(")
            || first_text.contains("parseInt")
            || first_text.contains("+process.env.PORT")
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.listen(process.env.PORT)` passes a string \u{2014} wrap with `Number(...)` or `parseInt(...)`.".into(),
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
    fn flags_raw_env_port() {
        let src = "import { Elysia } from 'elysia';\napp.listen(process.env.PORT);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_env_port_with_fallback_string() {
        let src = "import { Elysia } from 'elysia';\napp.listen(process.env.PORT ?? '3000');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_number_coercion() {
        let src = "import { Elysia } from 'elysia';\napp.listen(Number(process.env.PORT));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_parseint_coercion() {
        let src = "import { Elysia } from 'elysia';\napp.listen(parseInt(process.env.PORT ?? '3000', 10));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.listen(process.env.PORT);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
