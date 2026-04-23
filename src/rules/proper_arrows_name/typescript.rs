//! proper-arrows-name backend — flag anonymous arrow functions.
//!
//! An arrow is considered "named" (and thus allowed) when its immediate
//! parent gives JavaScript an inferred name:
//! - `variable_declarator` — `const foo = () => {}`
//! - `pair` — `{ foo: () => {} }`
//! - `public_field_definition` / `field_definition` — `class X { foo = () => {} }`
//! - `assignment_expression` where LHS is a member/identifier — `obj.foo = () => {}`
//!
//! Anything else (call argument, return value, array element, default-export
//! expression) is anonymous and gets flagged.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "arrow_function" {
        return;
    }
    let Some(parent) = node.parent() else {
        return;
    };
    if is_naming_parent(parent.kind()) {
        return;
    }
    // Assignment expression: only counts as naming if arrow is the RHS
    // (i.e. `x = () => {}`, not some nested position).
    if parent.kind() == "assignment_expression"
        && parent.child_by_field_name("right").map(|r| r.id()) == Some(node.id())
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "proper-arrows-name".into(),
        message: "Anonymous arrow function — assign it to a named binding so it \
                  appears by name in stack traces."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn is_naming_parent(kind: &str) -> bool {
    matches!(
        kind,
        "variable_declarator"
            | "pair"
            | "public_field_definition"
            | "field_definition"
            | "property_definition"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_named_const_arrow() {
        assert!(run_on("const foo = () => 1;").is_empty());
    }

    #[test]
    fn allows_object_property_arrow() {
        assert!(run_on("const o = { foo: () => 1 };").is_empty());
    }

    #[test]
    fn allows_class_field_arrow() {
        let src = "class Foo { bar = () => 1; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_member_assignment() {
        assert!(run_on("obj.foo = () => 1;").is_empty());
    }

    #[test]
    fn flags_callback_arrow() {
        let diags = run_on("[1, 2, 3].map(x => x * 2);");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "proper-arrows-name");
    }

    #[test]
    fn flags_iife_arrow() {
        let diags = run_on("(() => 42)();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_return_arrow() {
        let diags = run_on("function outer() { return () => 1; }");
        assert_eq!(diags.len(), 1);
    }
}
