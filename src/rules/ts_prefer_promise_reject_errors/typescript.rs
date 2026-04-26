//! ts-prefer-promise-reject-errors backend — flag `Promise.reject(<literal>)`.
//!
//! Flags `Promise.reject(arg)` where `arg` is a string, number, boolean,
//! `null`, `undefined`, object literal, array literal, template string,
//! or regex. Allows `new Error(...)`, identifiers, calls, and member
//! expressions which may evaluate to an `Error`.

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

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.utf8_text(source).unwrap_or("") != "Promise" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "reject" {
        return;
    }

    // Inspect the first argument.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(mut first) = args.named_child(0) else { return };

    while first.kind() == "parenthesized_expression" {
        match first.named_child(0) {
            Some(c) => first = c,
            None => return,
        }
    }

    if !is_non_error_value(first.kind()) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-prefer-promise-reject-errors".into(),
        message: "`Promise.reject()` should be called with an `Error` instance, \
                  not a primitive or object literal.".into(),
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
    fn flags_reject_string() {
        let d = run_on("Promise.reject('boom');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reject_number() {
        let d = run_on("Promise.reject(42);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reject_object_literal() {
        let d = run_on("Promise.reject({ code: 500 });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_reject_template_string() {
        let d = run_on("Promise.reject(`boom ${x}`);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_reject_new_error() {
        assert!(run_on("Promise.reject(new Error('boom'));").is_empty());
    }

    #[test]
    fn allows_reject_identifier() {
        assert!(run_on("Promise.reject(err);").is_empty());
    }

    #[test]
    fn allows_reject_call() {
        assert!(run_on("Promise.reject(makeError());").is_empty());
    }

    #[test]
    fn allows_promise_resolve() {
        assert!(run_on("Promise.resolve('value');").is_empty());
    }
}
