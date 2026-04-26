//! consistent-empty-array-spread AST backend — flag unparenthesized
//! ternaries in array spread: `[...condition ? ['a'] : []]`
//! → `[...(condition ? ['a'] : [])]`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["spread_element"] => |node, source, ctx, diagnostics|
    // The spread_element's child is the expression being spread.
    // If it's a ternary_expression, it's unparenthesized.
    // If it's a parenthesized_expression wrapping a ternary, it's OK.
    let Some(expr) = node.named_child(0) else { return };

    if expr.kind() == "ternary_expression" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "consistent-empty-array-spread".into(),
            message: "Parenthesize the ternary in array spread: \
                      `[...(condition ? ['a'] : [])]`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unparenthesized_ternary_spread() {
        assert_eq!(
            run_on("const arr = [...condition ? ['a'] : []];").len(),
            1
        );
    }

    #[test]
    fn allows_parenthesized_ternary_spread() {
        assert!(run_on("const arr = [...(condition ? ['a'] : [])];").is_empty());
    }

    #[test]
    fn flags_complex_condition() {
        assert_eq!(
            run_on("const arr = [...a && b ? [1] : []];").len(),
            1
        );
    }

    #[test]
    fn allows_normal_spread() {
        assert!(run_on("const arr = [...items];").is_empty());
    }

    #[test]
    fn allows_optional_chaining_spread() {
        assert!(run_on("const arr = [...obj?.items];").is_empty());
    }
}
