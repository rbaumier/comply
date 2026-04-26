//! no-instanceof-builtins backend — flag `x instanceof Array` and other builtins.

use crate::diagnostic::{Diagnostic, Severity};

/// Built-in constructors for which `instanceof` is unreliable.
const BUILTINS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "RegExp",
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
];

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    // Check for `instanceof` operator.
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");
    if op != "instanceof" {
        return;
    }

    // The right-hand side must be a bare identifier (a built-in name).
    let Some(right) = node.child_by_field_name("right") else { return };
    if right.kind() != "identifier" {
        return;
    }

    let name = right.utf8_text(source).unwrap_or("");
    if !BUILTINS.contains(&name) {
        return;
    }

    let suggestion = if name == "Array" {
        "Use `Array.isArray(x)` instead.".to_string()
    } else {
        format!("Avoid `instanceof {name}` — it fails across realms.")
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-instanceof-builtins".into(),
        message: suggestion,
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
    fn flags_instanceof_array() {
        let d = run_on("if (x instanceof Array) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.isArray"));
    }

    #[test]
    fn flags_instanceof_error() {
        let d = run_on("if (e instanceof Error) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("instanceof Error"));
    }

    #[test]
    fn flags_instanceof_promise() {
        let d = run_on("if (p instanceof Promise) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_instanceof_map() {
        let d = run_on("x instanceof Map");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_instanceof_set() {
        assert_eq!(run_on("x instanceof Set").len(), 1);
    }

    #[test]
    fn flags_instanceof_weakmap() {
        assert_eq!(run_on("x instanceof WeakMap").len(), 1);
    }

    #[test]
    fn flags_instanceof_regexp() {
        assert_eq!(run_on("x instanceof RegExp").len(), 1);
    }

    #[test]
    fn flags_instanceof_type_error() {
        assert_eq!(run_on("e instanceof TypeError").len(), 1);
    }

    #[test]
    fn allows_instanceof_custom_class() {
        assert!(run_on("if (x instanceof MyClass) {}").is_empty());
    }

    #[test]
    fn allows_instanceof_member_expression() {
        // `x instanceof foo.Bar` — right side is member_expression, not identifier.
        assert!(run_on("if (x instanceof foo.Bar) {}").is_empty());
    }

    #[test]
    fn allows_non_instanceof_binary() {
        assert!(run_on("x === Array").is_empty());
    }
}
