//! no-catch-log-rethrow backend — flag `catch (e) { log(e); throw e; }`
//! where the catch body contains only a logging call followed by a
//! `throw` of the same (or any) expression.
//!
//! Heuristic: the catch body's named children must be exactly two:
//!   1. an `expression_statement` whose expression is a call to one of
//!      `console.log/error/warn/info/debug`, `log`, `logger.*`,
//!      `Sentry.captureException`, etc.
//!   2. a `throw_statement`.
//!
//! We keep the logger list simple to limit false positives.

use crate::diagnostic::{Diagnostic, Severity};

const LOG_METHODS: &[&str] = &[
    "log", "error", "warn", "info", "debug", "trace", "captureException", "captureError",
];

const LOG_OBJECTS: &[&str] = &[
    "console", "logger", "log", "Sentry", "Rollbar", "Bugsnag", "tracer",
];

/// Is `node` a call expression whose callee looks like a logging call?
fn is_log_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).unwrap_or("");
            matches!(name, "log" | "error" | "warn" | "info" | "debug")
        }
        "member_expression" => {
            let Some(obj) = callee.child_by_field_name("object") else {
                return false;
            };
            let Some(prop) = callee.child_by_field_name("property") else {
                return false;
            };
            let obj_name = obj.utf8_text(source).unwrap_or("");
            let method = prop.utf8_text(source).unwrap_or("");
            LOG_OBJECTS.contains(&obj_name) && LOG_METHODS.contains(&method)
        }
        _ => false,
    }
}

crate::ast_check! { on ["catch_clause"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }
    if body.named_child_count() != 2 {
        return;
    }
    let stmt1 = match body.named_child(0) { Some(n) => n, None => return };
    let stmt2 = match body.named_child(1) { Some(n) => n, None => return };

    // First must be `expression_statement(call)` that looks like a log call.
    if stmt1.kind() != "expression_statement" {
        return;
    }
    let Some(expr) = stmt1.named_child(0) else { return };
    if !is_log_call(expr, source) {
        return;
    }

    // Second must be a throw statement.
    if stmt2.kind() != "throw_statement" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-catch-log-rethrow".into(),
        message: "Catch block only logs and rethrows — the top-level error handler \
                  already logs uncaught errors. Delete the catch, or add real value \
                  (wrap with context, recover, translate)."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
            run_on("try { x(); } catch (e) { throw new Error('boom', { cause: e }); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_log_without_rethrow() {
        assert!(run_on("try { x(); } catch (e) { console.error(e); }").is_empty());
    }

    #[test]
    fn allows_catch_with_extra_work() {
        assert!(run_on(
            "try { x(); } catch (e) { console.error(e); cleanup(); throw e; }"
        )
        .is_empty());
    }
}
