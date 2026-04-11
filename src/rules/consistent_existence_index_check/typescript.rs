//! consistent-existence-index-check — flag `< 0`, `>= 0`, `> -1` on index
//! methods. Prefer `=== -1` / `!== -1`.
//!
//! Detects both inline patterns (`foo.indexOf('x') < 0`) and variable-based
//! patterns (`const idx = arr.indexOf('x'); if (idx < 0) {}`).

use crate::diagnostic::{Diagnostic, Severity};

const INDEX_METHODS: &[&str] = &["indexOf", "lastIndexOf", "findIndex", "findLastIndex"];

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match binary expressions like `expr < 0`, `expr >= 0`, `expr > -1`
    if node.kind() != "binary_expression" {
        return;
    }

    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Check for `expr < 0`, `expr >= 0`, or `expr > -1`
    let is_bad = if ((op == "<" || op == ">=") && is_zero(&right, source))
        || (op == ">" && is_negative_one(&right, source))
    {
        is_index_expr(&left, source)
    } else {
        false
    };

    if !is_bad {
        return;
    }

    let message = if op == "<" {
        "Prefer `=== -1` over `< 0` to check index non-existence."
    } else {
        "Prefer `!== -1` over `>= 0` / `> -1` to check index existence."
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "consistent-existence-index-check".into(),
        message: message.into(),
        severity: Severity::Warning,
    });
}

/// Check if a node is a call to an index method: `expr.indexOf(...)` etc.
fn is_index_expr(node: &tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else {
                return false;
            };
            if func.kind() == "member_expression" {
                let Some(prop) = func.child_by_field_name("property") else {
                    return false;
                };
                let name = prop.utf8_text(source).unwrap_or("");
                return INDEX_METHODS.contains(&name);
            }
            false
        }
        // Also match identifiers that could be index variables — the AST
        // doesn't tell us what the variable holds, but the comparison
        // pattern `identifier < 0` is only meaningful for index results.
        // To avoid false positives, we only flag identifiers whose name
        // contains "index" or "idx" (case-insensitive).
        "identifier" => {
            let name = node.utf8_text(source).unwrap_or("");
            let lower = name.to_ascii_lowercase();
            lower.contains("index") || lower.contains("idx")
        }
        _ => false,
    }
}

/// Check if a node is the literal `0`.
fn is_zero(node: &tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "number" && node.utf8_text(source).unwrap_or("") == "0"
}

/// Check if a node is `-1` (a unary_expression with `-` and `1`).
fn is_negative_one(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "unary_expression" {
        let Some(op) = node.child_by_field_name("operator") else {
            return false;
        };
        let Some(arg) = node.child_by_field_name("argument") else {
            return false;
        };
        return op.utf8_text(source).unwrap_or("") == "-"
            && arg.kind() == "number"
            && arg.utf8_text(source).unwrap_or("") == "1";
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_inline_index_of_less_than_zero() {
        let d = run_on("if (foo.indexOf('bar') < 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("=== -1"));
    }

    #[test]
    fn flags_inline_index_of_gte_zero() {
        let d = run_on("if (foo.indexOf('bar') >= 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!== -1"));
    }

    #[test]
    fn flags_inline_index_of_gt_minus_one() {
        let d = run_on("if (foo.indexOf('bar') > -1) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!== -1"));
    }

    #[test]
    fn flags_find_last_index() {
        let d = run_on("if (arr.findLastIndex(x => x) > -1) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_last_index_of() {
        let d = run_on("if (str.lastIndexOf('a') < 0) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_triple_equals_minus_one() {
        assert!(run_on("if (foo.indexOf('bar') === -1) {}").is_empty());
    }

    #[test]
    fn allows_not_equals_minus_one() {
        assert!(run_on("if (foo.indexOf('bar') !== -1) {}").is_empty());
    }

    #[test]
    fn allows_unrelated_comparison() {
        assert!(run_on("if (count < 0) {}").is_empty());
    }
}
