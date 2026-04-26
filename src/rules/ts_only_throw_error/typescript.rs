//! ts-only-throw-error backend — flag `throw` of literal/object values.
//!
//! Flags:
//!   - `throw 'literal'` / `throw 42` / `throw true` / `throw null` / `throw undefined`
//!   - `throw { ... }` (object literal)
//!   - `throw [ ... ]` (array literal)
//!   - `throw \`template\``
//!
//! Allows:
//!   - `throw new Error(...)` / any `new` expression
//!   - `throw err` (identifier — may be an Error instance)
//!   - `throw fn()` (call expression — may return an Error)
//!   - `throw err.cause` (member expression)

use crate::diagnostic::{Diagnostic, Severity};

fn is_non_error_value(kind: &str) -> bool {
    matches!(
        kind,
        "string"
            | "template_string"
            | "number"
            | "true"
            | "false"
            | "null"
            | "undefined"
            | "object"
            | "array"
            | "regex"
    )
}

crate::ast_check! { on ["throw_statement"] => |node, _source, ctx, diagnostics|
    let Some(mut arg) = node.named_child(0) else { return };

    while arg.kind() == "parenthesized_expression" {
        match arg.named_child(0) {
            Some(c) => arg = c,
            None => return,
        }
    }

    if !is_non_error_value(arg.kind()) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-only-throw-error".into(),
        message: "Only throw `Error` instances — primitives and plain objects \
                  lose stack traces and break `instanceof` checks.".into(),
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
    fn flags_throw_string() {
        let d = run_on("function f() { throw 'boom'; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_number() {
        let d = run_on("function f() { throw 42; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_object_literal() {
        let d = run_on("function f() { throw { code: 500 }; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_template() {
        let d = run_on("function f() { throw `boom ${x}`; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_throw_new_error() {
        assert!(run_on("function f() { throw new Error('boom'); }").is_empty());
    }

    #[test]
    fn allows_throw_identifier() {
        assert!(run_on("function f(e) { throw e; }").is_empty());
    }

    #[test]
    fn allows_throw_call() {
        assert!(run_on("function f() { throw makeError(); }").is_empty());
    }
}
