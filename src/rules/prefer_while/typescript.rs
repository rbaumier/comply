//! prefer-while backend — flag `for(;;)` / `for(;cond;)` without init/update.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["for_statement"] => |node, source, ctx, diagnostics|
    // A for_statement has fields: initializer, condition, increment, body.
    // tree-sitter always provides `initializer` (as empty_statement when omitted).
    // `increment` is None when omitted.
    let has_init = node.child_by_field_name("initializer")
        .is_some_and(|n| n.kind() != "empty_statement");
    let has_increment = node.child_by_field_name("increment").is_some();

    if has_init || has_increment {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-while".into(),
        message: "Use `while` instead of `for` without init/update.".into(),
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
    fn flags_for_infinite() {
        assert_eq!(run_on("for (;;) {}").len(), 1);
    }

    #[test]
    fn flags_for_condition_only() {
        assert_eq!(run_on("for (;x < 10;) {}").len(), 1);
    }

    #[test]
    fn allows_standard_for_loop() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_while_true() {
        assert!(run_on("while (true) {}").is_empty());
    }
}
