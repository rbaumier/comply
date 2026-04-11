//! no-await-expression-member backend — flag member access on `(await expr)`.
//!
//! Detects `member_expression` nodes whose `object` is (possibly
//! parenthesized) `await_expression`. The pattern `(await fetch(url)).json()`
//! is hard to read; extracting the awaited value to a variable is clearer.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "member_expression" && node.kind() != "subscript_expression" {
        return;
    }

    let Some(object) = node.child_by_field_name("object") else { return };

    // Unwrap parenthesized_expression layers to find await_expression.
    let mut inner = object;
    while inner.kind() == "parenthesized_expression" {
        // The parenthesized child is the first named child.
        if let Some(child) = inner.named_child(0) {
            inner = child;
        } else {
            return;
        }
    }

    if inner.kind() != "await_expression" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-await-expression-member".into(),
        message: "Do not access a member directly from an await expression \
                  — extract to a variable first."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_member_access_on_await() {
        let d = run_on("const x = (await fetch(url)).json();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-await-expression-member");
    }

    #[test]
    fn flags_computed_member_on_await() {
        let d = run_on("const x = (await getItems())[0];");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_property_access_on_await() {
        let d = run_on("const x = (await getUser()).name;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_extracted_variable() {
        assert!(run_on("const res = await fetch(url); res.json();").is_empty());
    }

    #[test]
    fn allows_await_without_member_access() {
        assert!(run_on("const x = await fetch(url);").is_empty());
    }

    #[test]
    fn flags_chained_member_access() {
        // (await fetch(url)).headers.get('content-type')
        // The outer member_expression `.get(...)` has object `.headers`
        // which itself is a member_expression on await — so the inner one fires.
        let d = run_on("const x = (await fetch(url)).headers.get('ct');");
        assert!(!d.is_empty());
    }
}
