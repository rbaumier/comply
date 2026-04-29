//! no-useless-promise-resolve-reject backend — flag `return Promise.resolve(x)`
//! or `return Promise.reject(x)` inside async functions. In an async function,
//! `return x` is already `Promise.resolve(x)` and `throw x` is already
//! `Promise.reject(x)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Promise"] => |node, source, ctx, diagnostics|
    // Match `Promise.resolve(...)` and `Promise.reject(...)` call expressions.
    let Some(callee) = node.child_by_field_name("function") else { return };

    // Must be `Promise.resolve` or `Promise.reject` — a member_expression.
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(object) = callee.child_by_field_name("object") else { return };
    let Some(property) = callee.child_by_field_name("property") else { return };

    let obj_text = object.utf8_text(source).unwrap_or("");
    let prop_text = property.utf8_text(source).unwrap_or("");

    if obj_text != "Promise" {
        return;
    }
    if prop_text != "resolve" && prop_text != "reject" {
        return;
    }

    // The call must be the direct child of a return_statement or
    // the body of an arrow function expression.
    let Some(parent) = node.parent() else { return };

    let is_returned = match parent.kind() {
        "return_statement" => true,
        "arrow_function" => {
            // `=> Promise.resolve(x)` — the call IS the body (expression body).
            parent.child_by_field_name("body").map(|b| b.id()) == Some(node.id())
        }
        _ => false,
    };

    if !is_returned {
        return;
    }

    // Check if the enclosing function is async.
    // If the parent is already an arrow_function (expression body case),
    // check it directly. Otherwise walk up to find the enclosing function.
    let is_async = if parent.kind() == "arrow_function" {
        let t = parent.utf8_text(source).unwrap_or("");
        t.starts_with("async ")
    } else {
        let mut current = parent;
        loop {
            let Some(ancestor) = current.parent() else {
                break false;
            };
            match ancestor.kind() {
                "function_declaration" | "function" | "method_definition"
                | "arrow_function" | "generator_function_declaration" => {
                    let t = ancestor.utf8_text(source).unwrap_or("");
                    break t.starts_with("async ");
                }
                _ => {
                    current = ancestor;
                }
            }
        }
    };

    if !is_async {
        return;
    }

    let pos = callee.start_position();
    let replacement = if prop_text == "resolve" {
        "return the value directly"
    } else {
        "`throw` the error directly"
    };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-useless-promise-resolve-reject".into(),
        message: format!(
            "Unnecessary `Promise.{prop_text}()` in async function — {replacement}."
        ),
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

    // ---- flags violations ----

    #[test]
    fn flags_return_promise_resolve_in_async() {
        assert_eq!(
            run_on("async function f() { return Promise.resolve(1); }").len(),
            1
        );
    }

    #[test]
    fn flags_return_promise_reject_in_async() {
        assert_eq!(
            run_on("async function f() { return Promise.reject(new Error('x')); }").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_async_promise_resolve() {
        assert_eq!(run_on("const f = async () => Promise.resolve(1);").len(), 1);
    }

    #[test]
    fn flags_async_method_promise_resolve() {
        let src = "class A { async run() { return Promise.resolve(42); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // ---- allows correct usage ----

    #[test]
    fn allows_promise_resolve_in_non_async() {
        assert!(run_on("function f() { return Promise.resolve(1); }").is_empty());
    }

    #[test]
    fn allows_direct_return() {
        assert!(run_on("async function f() { return 1; }").is_empty());
    }

    #[test]
    fn allows_promise_all() {
        assert!(run_on("async function f() { return Promise.all([a, b]); }").is_empty());
    }
}
