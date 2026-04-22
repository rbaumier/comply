//! no-invalid-this backend — flag `this` tokens whose enclosing scope is
//! not a class method or a non-arrow function.
//!
//! Arrow functions inherit `this` from the enclosing scope, so an arrow
//! at the program top-level (or nested only inside arrows) has no legal
//! binding. A `this` inside a `function`/method is fine.

use crate::diagnostic::{Diagnostic, Severity};

fn has_valid_this_parent(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(parent) = cur {
        match parent.kind() {
            "method_definition"
            | "function_declaration"
            | "function"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "class_body"
            | "class_declaration"
            | "class"
            | "class_static_block" => return true,
            _ => {}
        }
        cur = parent.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "this" {
        return;
    }

    // Skip TypeScript type positions: `this` as a type (`this: void`,
    // `foo(this: Bar)`). Those never bind a runtime receiver.
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "type_annotation" | "this_type" | "predefined_type" => return,
            _ => {}
        }
    }

    if has_valid_this_parent(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-invalid-this".into(),
        message: "`this` is not bound in the current scope — move into a class method or a `function`.".into(),
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
    fn flags_this_at_top_level() {
        assert_eq!(run_on("this.foo = 1;").len(), 1);
    }

    #[test]
    fn flags_this_in_top_level_arrow() {
        assert_eq!(run_on("const f = () => this.foo;").len(), 1);
    }

    #[test]
    fn allows_this_in_class_method() {
        assert!(run_on("class A { m() { return this.x; } }").is_empty());
    }

    #[test]
    fn allows_this_in_function() {
        assert!(run_on("function f() { return this; }").is_empty());
    }

    #[test]
    fn allows_this_in_method_arrow() {
        assert!(run_on("class A { m() { const f = () => this.x; } }").is_empty());
    }
}
