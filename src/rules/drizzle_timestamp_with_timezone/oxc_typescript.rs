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
        Some(&["timestamp("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name != "timestamp" {
            return;
        }
        let arg_count = call.arguments.len();
        if arg_count >= 2 {
            return;
        }
        if arg_count == 1 {
            if let Some(expr) = call.arguments[0].as_expression() {
                if matches!(expr, Expression::ObjectExpression(_)) {
                    return;
                }
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`timestamp('col')` without `{ withTimezone: true }` \
                      — ambiguous across time zones. Always use \
                      `timestamp('col', { withTimezone: true })`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_bare_timestamp() {
        assert_eq!(run("const t = timestamp('created_at');").len(), 1);
    }

    #[test]
    fn allows_timestamp_with_options() {
        assert!(run("const t = timestamp('created_at', { withTimezone: true });").is_empty());
    }

    #[test]
    fn allows_timestamp_options_without_column_name() {
        assert!(run("const t = timestamp({ withTimezone: true });").is_empty());
    }
}
