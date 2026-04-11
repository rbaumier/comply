//! jsx-no-leaked-render backend — flag `{count && <X />}` patterns where
//! the left operand is not a boolean expression (may leak `0` or `""`).

use crate::diagnostic::{Diagnostic, Severity};

/// True if the identifier (last dot-segment) starts with a boolean-prefix.
fn likely_boolean(name: &str) -> bool {
    let segment = name.rsplit('.').next().unwrap_or(name);
    let lower = segment.to_lowercase();
    const PREFIXES: &[&str] = &[
        "is", "has", "should", "can", "will", "did", "show", "hide",
        "enable", "disable", "visible", "active", "open", "loading", "loaded",
    ];
    PREFIXES.iter().any(|p| lower.starts_with(p))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look for `&&` binary expressions inside JSX expression containers.
    if node.kind() != "jsx_expression" {
        return;
    }

    // Find a binary_expression child with `&&`.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "binary_expression" {
            continue;
        }

        // Check operator is `&&`.
        let Some(op) = child.child_by_field_name("operator") else { continue };
        let Ok(op_text) = op.utf8_text(source) else { continue };
        if op_text != "&&" {
            continue;
        }

        // Check that the right side contains JSX.
        let Some(right) = child.child_by_field_name("right") else { continue };
        let right_kind = right.kind();
        let has_jsx = right_kind == "jsx_element"
            || right_kind == "jsx_self_closing_element"
            || right_kind == "jsx_fragment";
        if !has_jsx {
            continue;
        }

        // Check the left side: if it's a boolean coercion (`!!x`) or comparison, skip.
        let Some(left) = child.child_by_field_name("left") else { continue };

        // `!!x` — unary_expression wrapping another unary_expression with `!`
        if left.kind() == "unary_expression" {
            let Ok(left_text) = left.utf8_text(source) else { continue };
            if left_text.starts_with("!!") {
                continue;
            }
        }

        // Comparison operators produce booleans — skip.
        if left.kind() == "binary_expression"
            && let Some(inner_op) = left.child_by_field_name("operator") {
                let Ok(inner_op_text) = inner_op.utf8_text(source) else { continue };
                match inner_op_text {
                    ">" | "<" | ">=" | "<=" | "==" | "===" | "!=" | "!==" => continue,
                    _ => {}
                }
            }

        // Check if the identifier is likely a boolean name.
        let Ok(left_text) = left.utf8_text(source) else { continue };
        if likely_boolean(left_text.trim()) {
            continue;
        }

        let pos = child.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "jsx-no-leaked-render".into(),
            message: "Potential leaked render — numeric/string value with `&&` renders \
                      falsy value (`0`, `\"\"`) instead of nothing."
                .into(),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_count_and_jsx() {
        let src = "const x = <div>{count && <Component />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_length_and_jsx() {
        let src = "const x = <div>{items.length && <List />}</div>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_double_bang() {
        let src = "const x = <div>{!!count && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_comparison() {
        let src = "const x = <div>{count > 0 && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_prefix() {
        let src = "const x = <div>{isReady && <Component />}</div>;";
        assert!(run_on(src).is_empty());
    }
}
