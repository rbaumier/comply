//! prefer-regexp-test backend — flag `.match(/regex/)` in boolean contexts.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if the parent node represents a boolean context (if, while, ternary,
/// unary `!`, logical `&&`/`||`).
fn is_boolean_context(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    match parent.kind() {
        "if_statement" | "while_statement" | "do_statement" => {
            // The call must be the condition, not the body
            parent
                .child_by_field_name("condition")
                .is_some_and(|c| c.id() == node.id())
        }
        "unary_expression" => {
            // `!str.match(...)` or `!!str.match(...)`
            true
        }
        "binary_expression" => {
            // `str.match(...) && x` or `x || str.match(...)`
            let Some(op) = parent.child_by_field_name("operator") else { return false };
            let op_text = op.kind();
            op_text == "&&" || op_text == "||"
        }
        "parenthesized_expression" => {
            // Recurse up: `if ((str.match(...)))`
            is_boolean_context(parent)
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "match" {
        return;
    }

    // Check that the first argument is a regex literal
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let has_regex_arg = args.children(&mut cursor).any(|c| c.kind() == "regex");

    if !has_regex_arg {
        return;
    }

    // Only flag if in a boolean context
    if !is_boolean_context(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-regexp-test".into(),
        message: "Prefer `RegExp#test()` over `String#match()` in boolean contexts.".into(),
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
    fn flags_match_in_if() {
        let d = run_on("if (str.match(/foo/)) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-regexp-test");
    }

    #[test]
    fn flags_match_with_double_bang() {
        let d = run_on("const ok = !!str.match(/bar/);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_match_outside_boolean() {
        assert!(run_on("const m = str.match(/foo/);").is_empty());
    }

    #[test]
    fn allows_match_with_variable() {
        assert!(run_on("if (str.match(pattern)) {}").is_empty());
    }

    #[test]
    fn allows_test_call() {
        assert!(run_on("if (/foo/.test(str)) {}").is_empty());
    }
}
