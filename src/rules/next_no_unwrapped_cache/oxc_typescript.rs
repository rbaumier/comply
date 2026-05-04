use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
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
        if ctx.project.framework != Framework::NextJs {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if callee_name != "unstable_cache" && callee_name != "cache" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };

        let is_inline_fn = matches!(
            expr,
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        );
        if !is_inline_fn {
            return;
        }

        // Check if the body text contains "try"
        let arg_span = expr.span();
        let arg_text = &ctx.source[arg_span.start as usize..arg_span.end as usize];
        if arg_text.contains("try") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        let range_start = call.span.start as usize;
        let range_len = (call.span.end - call.span.start) as usize;
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "next-no-unwrapped-cache".into(),
            message: format!(
                "`{callee_name}` callback has no try/catch — an unhandled throw will poison the cache."
            ),
            severity: Severity::Warning,
            span: Some((range_start, range_len)),
        });
    }
}
