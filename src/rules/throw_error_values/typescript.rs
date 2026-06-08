//! throw-error-values backend — flag `throw` of non-Error values.
//!
//! Flags:
//!   - `throw 'literal'` / `throw 42` / `throw true` / `throw null` / `throw undefined`
//!   - `throw { ... }` (object literal)
//!   - `throw [ ... ]` (array literal)
//!   - `throw \`template\``
//!
//! Allows:
//!   - `throw new Error(...)` / `throw new TypeError(...)` / any `new` expression
//!   - `throw err` (identifier — may be an Error instance)
//!   - `throw fn()` (call expression — may return an Error)
//!   - `throw err.cause` (member expression — may be an Error)

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
    // throw_statement's argument is the first named child.
    let Some(mut arg) = node.named_child(0) else { return };

    // Unwrap parenthesized_expression.
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
        rule_id: "throw-error-values".into(),
        message: "Throw an `Error` instance, not a primitive or plain object — \
                  non-Error throws lose stack traces and break `instanceof` checks."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_throw_string_literal() {
        let d = run_on("function f() { throw 'boom'; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "throw-error-values");
    }

    #[test]
    fn flags_throw_template_string() {
        let d = run_on("function f() { throw `boom ${x}`; }");
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
    fn flags_throw_array() {
        let d = run_on("function f() { throw [1, 2]; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_throw_null() {
        let d = run_on("function f() { throw null; }");
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
    fn allows_throw_call_expression() {
        assert!(run_on("function f() { throw makeError(); }").is_empty());
    }

    #[test]
    fn allows_throw_member() {
        assert!(run_on("function f(e) { throw e.cause; }").is_empty());
    }
}
