//! no-built-in-override backend — flag variable declarations shadowing built-in globals.

use crate::diagnostic::{Diagnostic, Severity};

const BUILTINS: &[&str] = &[
    "Array", "Object", "String", "Map", "Set", "Promise", "JSON", "Math",
    "undefined", "NaN", "Infinity",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "variable_declarator" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    if name_node.kind() != "identifier" {
        return;
    }
    let name = name_node.utf8_text(source).unwrap_or("");
    if !BUILTINS.contains(&name) {
        return;
    }

    // Must have a value (an assignment, not just a declaration).
    if node.child_by_field_name("value").is_none() {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-built-in-override".into(),
        message: format!("Overriding built-in `{}` — rename this variable.", name),
        severity: Severity::Error,
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
    fn flags_const_array_override() {
        let d = run_on("const Array = [];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array"));
    }

    #[test]
    fn flags_let_object_override() {
        let d = run_on("let Object = {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object"));
    }

    #[test]
    fn flags_promise_override() {
        assert_eq!(run_on("const Promise = null;").len(), 1);
    }

    #[test]
    fn flags_undefined_override() {
        let d = run_on("const undefined = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("undefined"));
    }

    #[test]
    fn allows_normal_variables() {
        assert!(run_on("const myArray = [];").is_empty());
    }

    #[test]
    fn allows_usage_not_assignment() {
        assert!(run_on("const x = Array.from([1, 2, 3]);").is_empty());
    }
}
