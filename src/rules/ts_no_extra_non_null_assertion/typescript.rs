//! ts-no-extra-non-null-assertion backend — flag nested `non_null_expression`
//! nodes (i.e. `x!!`).
//!
//! Detection: walk `non_null_expression` nodes whose inner expression is also
//! a `non_null_expression`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["non_null_expression"] => |node, _source, ctx, diagnostics|
    let Some(inner) = node.named_child(0) else {
        return;
    };
    if inner.kind() != "non_null_expression" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-extra-non-null-assertion".into(),
        message: "Extra non-null assertion — `x!!` is redundant, use `x!`.".into(),
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
    fn flags_double_bang_on_expression() {
        let diags = run_on("const x = value!!;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_single_non_null_assertion() {
        assert!(run_on("const x = value!;").is_empty());
    }

    #[test]
    fn allows_boolean_coercion() {
        assert!(run_on("const x = !!value;").is_empty());
    }

    #[test]
    fn flags_triple_bang() {
        let diags = run_on("const x = value!!!;");
        // triple produces nested non_null_expression nodes
        assert!(!diags.is_empty());
    }
}
