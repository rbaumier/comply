//! no-invalid-void-type backend — flag `void` used outside of return
//! type annotations and generic type arguments.
//!
//! Detection: walk `predefined_type` nodes whose source text is `void`.
//! Allow when parent is a return type annotation of a function or a
//! generic type argument (including `void` within a union return).

use crate::diagnostic::{Diagnostic, Severity};

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "function_signature"
            | "method_signature"
            | "abstract_method_definition"
            | "call_signature"
            | "construct_signature"
            | "generator_function_declaration"
            | "generator_function"
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "predefined_type" {
        return;
    }
    let text = &source[node.byte_range()];
    if text != b"void" {
        return;
    }
    let Some(parent) = node.parent() else { return };

    // Allow: direct return type annotation on a function-like node.
    if parent.kind() == "type_annotation"
        && let Some(grandparent) = parent.parent()
        && is_function_like(grandparent.kind())
    {
        return;
    }

    // Allow: generic type argument (`Promise<void>`).
    if parent.kind() == "type_arguments" {
        return;
    }

    // Allow: `void` inside a union type that sits in a return-type position.
    if parent.kind() == "union_type"
        && let Some(grandparent) = parent.parent()
        && grandparent.kind() == "type_annotation"
        && let Some(great) = grandparent.parent()
        && is_function_like(great.kind())
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-invalid-void-type".into(),
        message: "`void` is only valid as a return type or generic type argument.".into(),
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
    fn flags_void_variable() {
        assert_eq!(run_on("let x: void;").len(), 1);
    }

    #[test]
    fn flags_void_parameter() {
        assert_eq!(run_on("function foo(x: void) {}").len(), 1);
    }

    #[test]
    fn allows_void_return_type() {
        assert!(run_on("function foo(): void {}").is_empty());
    }

    #[test]
    fn allows_void_in_generic() {
        assert!(run_on("let x: Promise<void>;").is_empty());
    }

    #[test]
    fn allows_void_in_union_return() {
        assert!(run_on("function foo(): void | string { return ''; }").is_empty());
    }
}
