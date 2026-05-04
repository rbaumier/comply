//! no-catch-log-rethrow oxc backend — flag `catch (e) { log(e); throw e; }`
//! where the catch body is exactly a logging call followed by a throw.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const LOG_METHODS: &[&str] = &[
    "log",
    "error",
    "warn",
    "info",
    "debug",
    "trace",
    "captureException",
    "captureError",
];

const LOG_OBJECTS: &[&str] = &[
    "console", "logger", "log", "Sentry", "Rollbar", "Bugsnag", "tracer",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };
        let Some(ref handler) = try_stmt.handler else {
            return;
        };
        let body = &handler.body.body;
        if body.len() != 2 {
            return;
        }

        use oxc_ast::ast::Statement;

        // First statement must be an expression statement with a log call.
        let Statement::ExpressionStatement(expr_stmt) = &body[0] else {
            return;
        };
        if !is_log_call(&expr_stmt.expression, ctx.source) {
            return;
        }

        // Second statement must be a throw.
        let Statement::ThrowStatement(_) = &body[1] else {
            return;
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, handler.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Catch block only logs and rethrows — the top-level error handler \
                      already logs uncaught errors. Delete the catch, or add real value \
                      (wrap with context, recover, translate)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_log_call(expr: &oxc_ast::ast::Expression, _source: &str) -> bool {
    use oxc_ast::ast::Expression;

    let Expression::CallExpression(call) = expr else {
        return false;
    };

    match &call.callee {
        Expression::Identifier(id) => {
            matches!(
                id.name.as_str(),
                "log" | "error" | "warn" | "info" | "debug"
            )
        }
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            let obj_name = obj.name.as_str();
            let method = member.property.name.as_str();
            LOG_OBJECTS.contains(&obj_name) && LOG_METHODS.contains(&method)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_console_error_then_throw() {
        let d = run_on("try { x(); } catch (e) { console.error(e); throw e; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-catch-log-rethrow");
    }

    #[test]
    fn flags_logger_error_then_throw() {
        let d = run_on("try { x(); } catch (e) { logger.error(e); throw e; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_log_then_throw() {
        let d = run_on("try { x(); } catch (e) { log(e); throw e; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sentry_capture_then_throw() {
        let d = run_on("try { x(); } catch (e) { Sentry.captureException(e); throw e; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_catch_with_wrap() {
        assert!(
            run_on("try { x(); } catch (e) { throw new Error('boom', { cause: e }); }").is_empty()
        );
    }

    #[test]
    fn allows_log_without_rethrow() {
        assert!(run_on("try { x(); } catch (e) { console.error(e); }").is_empty());
    }

    #[test]
    fn allows_catch_with_extra_work() {
        assert!(
            run_on("try { x(); } catch (e) { console.error(e); cleanup(); throw e; }").is_empty()
        );
    }
}
