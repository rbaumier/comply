//! no-arguments-usage backend — flag direct use of the `arguments` object.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match `arguments` used as an identifier in member expressions or subscripts.
    if node.kind() != "identifier" {
        return;
    }
    let Ok(text) = node.utf8_text(source) else { return };
    if text != "arguments" {
        return;
    }

    // Only flag when used as the object of a member/subscript expression,
    // e.g. `arguments[0]`, `arguments.length`, `arguments.callee`.
    let Some(parent) = node.parent() else { return };
    match parent.kind() {
        "member_expression" | "subscript_expression" => {
            // Ensure `arguments` is the object, not the property.
            if parent.child_by_field_name("object") != Some(node) {
                return;
            }
        }
        _ => return,
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-arguments-usage".into(),
        message: "Avoid direct use of `arguments` — use rest parameters (`...args`) instead."
            .into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_arguments_bracket() {
        assert_eq!(run_on("function f() { return arguments[0]; }").len(), 1);
    }

    #[test]
    fn flags_arguments_length() {
        assert_eq!(
            run_on("function f() { if (arguments.length > 0) {} }").len(),
            1
        );
    }

    #[test]
    fn flags_arguments_callee() {
        assert_eq!(
            run_on("function f() { return arguments.callee; }").len(),
            1
        );
    }

    #[test]
    fn allows_rest_params() {
        assert!(run_on("function foo(...args: any[]) { return args[0]; }").is_empty());
    }

    #[test]
    fn allows_unrelated_identifier() {
        assert!(run_on("const arguments_list = [1, 2, 3];").is_empty());
    }
}
