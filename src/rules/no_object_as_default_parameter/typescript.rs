//! no-object-as-default-parameter backend — flag `function f(opts = { key: val })`.
//!
//! Walks the AST looking for function parameters with default values that are
//! non-empty object literals. Tree-sitter models default parameters as
//! `assignment_pattern` nodes (for plain JS/TS params) inside function
//! parameter lists.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the given node is a function-like node.
fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration"
            | "generator_function"
    )
}

/// Returns true if this assignment_pattern is a direct child of a function's
/// formal_parameters (possibly wrapped in a required/optional_parameter for TS).
fn is_param_default(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    match parent.kind() {
        "formal_parameters" => {
            // Check grandparent is function-like.
            parent
                .parent()
                .is_some_and(|gp| is_function_like(gp.kind()))
        }
        // TypeScript wraps params: required_parameter / optional_parameter
        // contain the assignment_pattern, and their parent is formal_parameters.
        "required_parameter" | "optional_parameter" => {
            let Some(gp) = parent.parent() else {
                return false;
            };
            if gp.kind() != "formal_parameters" {
                return false;
            }
            gp.parent()
                .is_some_and(|ggp| is_function_like(ggp.kind()))
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Handle both `assignment_pattern` (plain JS) and
    // `required_parameter`/`optional_parameter` (TypeScript) with a default value.
    let (left_field, right_field) = match node.kind() {
        "assignment_pattern" => {
            if !is_param_default(node) {
                return;
            }
            ("left", "right")
        }
        "required_parameter" | "optional_parameter" => {
            // Only inside a formal_parameters of a function-like node.
            let Some(parent) = node.parent() else { return };
            if parent.kind() != "formal_parameters" {
                return;
            }
            if !parent.parent().is_some_and(|gp| is_function_like(gp.kind())) {
                return;
            }
            // Must have a `value` field (the default).
            if node.child_by_field_name("value").is_none() {
                return;
            }
            ("pattern", "value")
        }
        _ => return,
    };

    // The right-hand side is the default value.
    let Some(right) = node.child_by_field_name(right_field) else { return };
    if right.kind() != "object" {
        return;
    }

    // Only flag non-empty object literals — `= {}` is fine.
    if right.named_child_count() == 0 {
        return;
    }

    let left = node.child_by_field_name(left_field);
    let param_name = left
        .filter(|l| l.kind() == "identifier")
        .and_then(|l| l.utf8_text(source).ok());

    let pos = node.start_position();
    let message = match param_name {
        Some(name) => format!(
            "Do not use an object literal as default for parameter `{name}`. \
             Use destructuring with individual defaults instead."
        ),
        None => "Do not use an object literal as default. \
                 Use destructuring with individual defaults instead."
            .to_string(),
    };

    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-object-as-default-parameter".into(),
        message,
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
    fn flags_object_default_in_function() {
        let d = run_on("function f(opts = { timeout: 1000 }) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("opts"));
    }

    #[test]
    fn flags_object_default_in_arrow() {
        let d = run_on("const f = (opts = { retries: 3 }) => {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("opts"));
    }

    #[test]
    fn flags_object_default_in_method() {
        let d = run_on("class A { method(cfg = { debug: true }) {} }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_empty_object_default() {
        assert!(run_on("function f(opts = {}) {}").is_empty());
    }

    #[test]
    fn allows_destructured_default() {
        assert!(run_on("function f({ timeout = 1000 } = {}) {}").is_empty());
    }

    #[test]
    fn allows_primitive_default() {
        assert!(run_on("function f(x = 42) {}").is_empty());
    }

    #[test]
    fn allows_array_default() {
        assert!(run_on("function f(items = [1, 2]) {}").is_empty());
    }

    #[test]
    fn allows_assignment_in_body() {
        // Assignment patterns inside function body are NOT parameter defaults.
        assert!(run_on("function f() { const x = { a: 1 }; }").is_empty());
    }
}
