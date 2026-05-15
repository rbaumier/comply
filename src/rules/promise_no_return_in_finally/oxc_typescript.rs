//! promise-no-return-in-finally oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

pub struct Check;

fn body_returns_value(body: &oxc_ast::ast::FunctionBody) -> bool {
    body.statements.iter().any(|s| {
        matches!(s, Statement::ReturnStatement(ret) if ret.argument.is_some())
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".finally("])
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
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "finally" {
            return;
        }
        let Some(arg) = call.arguments.first() else {
            return;
        };
        let returns_value = match arg {
            Argument::ArrowFunctionExpression(a) => {
                if a.expression {
                    // Concise body returning a value (`.finally(() => x)`) —
                    // less obvious bug; the return value is discarded.
                    !matches!(
                        a.body.statements.first(),
                        Some(Statement::ExpressionStatement(_))
                    ) || a
                        .body
                        .statements
                        .first()
                        .map(|s| !matches!(s, Statement::ExpressionStatement(_)))
                        .unwrap_or(false)
                        || a.body.statements.first().is_some()
                } else {
                    body_returns_value(&a.body)
                }
            }
            Argument::FunctionExpression(f) => f
                .body
                .as_ref()
                .is_some_and(|b| body_returns_value(b)),
            _ => false,
        };
        // For concise body, treat any non-trivial expression as a return.
        let returns_value = match arg {
            Argument::ArrowFunctionExpression(a) if a.expression => true,
            _ => returns_value,
        };
        if !returns_value {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`return` in `.finally(...)` is discarded — move the value to a \
                      preceding `.then(...)` if it matters."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_return_in_finally_block() {
        let src = "p.finally(() => { return cleanup(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_concise_arrow_finally() {
        let src = "p.finally(() => cleanup());";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_finally_with_side_effect_only() {
        let src = "p.finally(() => { cleanup(); });";
        assert!(run(src).is_empty());
    }
}
