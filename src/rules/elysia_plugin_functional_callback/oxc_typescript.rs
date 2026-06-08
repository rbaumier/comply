//! elysia-plugin-functional-callback oxc backend — flag arrow plugins
//! typed as `(app: Elysia) => ...`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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

        // Get the function's full source text and extract params/body text.
        let (params_text, body_text, span_start) = match node.kind() {
            AstKind::ArrowFunctionExpression(arrow) => {
                let params_span = arrow.params.span;
                let params_text = &ctx.source[params_span.start as usize..params_span.end as usize];
                let body_span = arrow.body.span;
                let body_text = &ctx.source[body_span.start as usize..body_span.end as usize];
                (params_text, body_text, arrow.span.start)
            }
            AstKind::Function(func) => {
                let params_span = func.params.span;
                let params_text = &ctx.source[params_span.start as usize..params_span.end as usize];
                let Some(body) = &func.body else { return };
                let body_text = &ctx.source[body.span.start as usize..body.span.end as usize];
                (params_text, body_text, func.span.start)
            }
            _ => return,
        };

        // Single param annotated with `: Elysia`.
        if !params_text.contains(": Elysia") && !params_text.contains(":Elysia")  {
            return;
        }

        // Extract param name from `(name: Elysia)`.
        let pname = params_text
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split(':')
            .next()
            .unwrap_or("")
            .trim();
        if pname.is_empty() {
            return;
        }

        // Heuristic: the body chains methods on the parameter.
        let chain_marker = format!("{pname}.");
        if !body_text.contains(&chain_marker) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Functional plugin `(app: Elysia) => ...` loses type inference — return a `new Elysia()` instance instead.".into(),
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
    fn flags_arrow_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = (app: Elysia) => app.get('/x', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_function_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport function plugin(app: Elysia) { return app.get('/x', () => 'ok'); }";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_instance_plugin() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia({ name: 'plugin' }).get('/x', () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "export const plugin = (app: Elysia) => app.get('/x', () => 'ok');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
