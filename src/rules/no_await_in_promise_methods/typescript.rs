//! no-await-in-promise-methods backend — flag `await` inside Promise method arrays.
//!
//! Detects patterns like `Promise.all([await fetchA(), await fetchB()])`.
//! The `await` serializes the calls, defeating the purpose of `Promise.all()`.
//!
//! Strategy: find `call_expression` nodes where the callee is
//! `Promise.{all,allSettled,any,race}`, the first argument is an array,
//! and any array element is an `await_expression`.

use crate::diagnostic::{Diagnostic, Severity};

const PROMISE_METHODS: &[&str] = &["all", "allSettled", "any", "race"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // callee must be `Promise.method`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.kind() != "identifier" || obj.utf8_text(source).unwrap_or("") != "Promise" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method_name = prop.utf8_text(source).unwrap_or("");
    if !PROMISE_METHODS.contains(&method_name) {
        return;
    }

    // First argument must be an array
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 1 {
        return;
    }
    let Some(first_arg) = args.named_child(0) else { return };
    if first_arg.kind() != "array" {
        return;
    }

    // Walk array elements looking for await_expression
    let child_count = first_arg.named_child_count();
    for i in 0..child_count {
        let Some(element) = first_arg.named_child(i) else { continue };
        if element.kind() == "await_expression" {
            let pos = element.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-await-in-promise-methods".into(),
                message: format!(
                    "Promise in `Promise.{method_name}()` should not be awaited \
                     — this serializes the calls."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_await_in_promise_all() {
        let d = run_on("await Promise.all([await fetchA(), await fetchB()]);");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].rule_id, "no-await-in-promise-methods");
    }

    #[test]
    fn flags_single_await_in_promise_race() {
        let d = run_on("await Promise.race([await fetchA(), fetchB()]);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_await_in_promise_all_settled() {
        let d = run_on("await Promise.allSettled([await a(), await b()]);");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn flags_await_in_promise_any() {
        let d = run_on("await Promise.any([await fetchA()]);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_no_await_in_promise_all() {
        assert!(run_on("await Promise.all([fetchA(), fetchB()]);").is_empty());
    }

    #[test]
    fn allows_promise_resolve() {
        assert!(run_on("await Promise.resolve(42);").is_empty());
    }

    #[test]
    fn allows_non_promise_call() {
        assert!(run_on("foo([await bar()]);").is_empty());
    }
}
