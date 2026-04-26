//! ts-no-non-null-asserted-nullish-coalescing backend — flag
//! `non_null_expression` nodes that appear as the left operand of a `??`
//! binary expression.
//!
//! Detection: walk `non_null_expression` nodes and check if their parent
//! is a binary_expression with the `??` operator.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["non_null_expression"] => |node, source, ctx, diagnostics|
    let Some(parent) = node.parent() else {
        return;
    };
    if parent.kind() != "binary_expression" {
        return;
    }
    // Check that this non_null_expression is the left operand
    let Some(left) = parent.child_by_field_name("left") else {
        return;
    };
    if left.id() != node.id() {
        return;
    }
    // Check operator is `??`
    let Some(op_node) = parent.child_by_field_name("operator") else {
        return;
    };
    let op_text = &source[op_node.byte_range()];
    if op_text != b"??" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-non-null-asserted-nullish-coalescing".into(),
        message: "`x! ?? y` is contradictory — the `!` asserts non-null \
                  while `??` handles null. Remove the `!`."
            .into(),
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
    fn flags_non_null_with_nullish_coalescing() {
        let diags = run_on("const x = value! ?? 'default';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_nullish_coalescing_without_non_null() {
        assert!(run_on("const x = value ?? 'default';").is_empty());
    }

    #[test]
    fn allows_non_null_without_nullish_coalescing() {
        assert!(run_on("const x = value!;").is_empty());
    }
}
