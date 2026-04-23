//! top-level-function backend — flag top-level `const foo = () => {...}`.
//!
//! A `variable_declarator` whose value is an `arrow_function` counts as a
//! named top-level function in disguise. Stack traces show `<anonymous>`,
//! the binding isn't hoisted, and refactoring to `function` unlocks method
//! shorthand on re-exports.
//!
//! Detection walks `variable_declarator` nodes whose grand-parent is
//! `program`. The parent between the declarator and `program` is the
//! enclosing `lexical_declaration` / `variable_declaration`. `export`
//! statements wrap the declaration in one more level, so we also accept
//! `program → export_statement → lexical_declaration → variable_declarator`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "variable_declarator" {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else {
        return;
    };
    if value.kind() != "arrow_function" {
        return;
    }

    // Parent is the declaration (`lexical_declaration` or
    // `variable_declaration`). Walk up past an optional `export_statement`.
    let Some(decl) = node.parent() else { return };
    if decl.kind() != "lexical_declaration" && decl.kind() != "variable_declaration" {
        return;
    }
    let Some(outer) = decl.parent() else { return };
    let top_container = match outer.kind() {
        "program" => outer,
        "export_statement" => {
            let Some(gp) = outer.parent() else { return };
            gp
        }
        _ => return,
    };
    if top_container.kind() != "program" {
        return;
    }

    // Extract the variable name for the message.
    let name = node
        .child_by_field_name("name")
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("<unknown>");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "top-level-function".into(),
        message: format!(
            "Top-level `const {name} = () => ...` — prefer `function {name}(...) {{ ... }}` \
             for a named binding, hoisting, and better stack traces."
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

    #[test]
    fn flags_top_level_const_arrow() {
        let diags = run_on("const foo = () => 42;");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "top-level-function");
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn flags_top_level_let_arrow() {
        let diags = run_on("let foo = () => 42;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_exported_top_level_arrow() {
        let diags = run_on("export const foo = () => 42;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_function_declaration() {
        assert!(run_on("function foo() { return 42; }").is_empty());
    }

    #[test]
    fn allows_nested_arrow() {
        // Inside a function body — not top-level.
        let src = "function outer() { const inner = () => 1; return inner; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_function_top_level_const() {
        assert!(run_on("const x = 42;").is_empty());
    }

    #[test]
    fn allows_arrow_as_callback() {
        let src = "[1, 2, 3].map(x => x * 2);";
        assert!(run_on(src).is_empty());
    }
}
