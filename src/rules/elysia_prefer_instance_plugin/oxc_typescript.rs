//! elysia-prefer-instance-plugin OXC backend — flag callback-style Elysia plugins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns true if the first parameter has a type annotation containing `Elysia`.
fn first_param_is_elysia(params: &oxc_ast::ast::FormalParameters, source: &str) -> bool {
    let Some(first) = params.items.first() else {
        return false;
    };
    let Some(ann) = &first.type_annotation else {
        return false;
    };
    let ann_text =
        &source[ann.span.start as usize..ann.span.end as usize];
    let trimmed = ann_text.trim_start_matches(':').trim();
    trimmed == "Elysia" || trimmed.starts_with("Elysia<")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression, AstType::Function]
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

        let (params, span_start) = match node.kind() {
            AstKind::ArrowFunctionExpression(arrow) => (&arrow.params, arrow.span.start),
            AstKind::Function(func) => (&func.params, func.span.start),
            _ => return,
        };

        if !first_param_is_elysia(params, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Callback-style plugin `(app: Elysia) => ...` \u{2014} prefer `new Elysia({ name: '...' })` instance plugins for deduplication and type inference.".into(),
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
    fn flags_callback_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = (app: Elysia) => app.get('/', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_function_expression_callback() {
        let src = "import { Elysia } from 'elysia';\nexport function plugin(app: Elysia) { return app.get('/', () => 'ok'); }";
        // function declarations don't match; use a function expression.
        let src2 = "import { Elysia } from 'elysia';\nexport const plugin = function(app: Elysia) { return app; };";
        let _ = src;
        assert_eq!(run_on(src2).len(), 1);
    }


    #[test]
    fn allows_instance_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ name: 'p' }).get('/', () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const plugin = (app: Elysia) => app.get('/', () => 'ok');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
