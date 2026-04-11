//! no-lonely-if backend — flag `else { if (x) { } }` that should be
//! `else if (x) { }`.
//!
//! This rule specifically targets an `if_statement` that is the SOLE
//! statement inside an `else_clause`'s `statement_block`. When the else
//! block contains only an if (with or without its own else), the outer
//! braces add needless nesting.
//!
//! NOT the same as `no-collapsible-if` which merges `if (a) { if (b) {} }`
//! into `if (a && b) {}`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "if_statement" {
        return;
    }

    // Check: is this if_statement the sole child of a statement_block
    // that is inside an else_clause?
    let Some(parent) = node.parent() else { return };

    if parent.kind() != "statement_block" {
        return;
    }

    // The statement_block must contain exactly one named child (this if)
    if parent.named_child_count() != 1 {
        return;
    }

    // The statement_block must be inside an else_clause
    let Some(grandparent) = parent.parent() else { return };
    if grandparent.kind() != "else_clause" {
        return;
    }

    // The else_clause must belong to an if_statement
    let Some(great_grandparent) = grandparent.parent() else { return };
    if great_grandparent.kind() != "if_statement" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-lonely-if".into(),
        message: "Unexpected `if` as the only statement in an `else` block \
                  — use `else if` instead."
            .into(),
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
    fn flags_lonely_if_in_else() {
        let src = r#"
if (a) {
    foo();
} else {
    if (b) {
        bar();
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-lonely-if");
    }

    #[test]
    fn flags_lonely_if_else_in_else() {
        // The inner if has its own else — still flaggable since
        // it should be `else if (b) { ... } else { ... }`
        let src = r#"
if (a) {
    foo();
} else {
    if (b) {
        bar();
    } else {
        baz();
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_else_if() {
        let src = r#"
if (a) {
    foo();
} else if (b) {
    bar();
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_with_multiple_statements() {
        let src = r#"
if (a) {
    foo();
} else {
    doSetup();
    if (b) {
        bar();
    }
}
"#;
        // The else block has 2 statements, so the if is not "lonely"
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_if_without_else() {
        // This is `no-collapsible-if` territory, not `no-lonely-if`
        let src = r#"
if (a) {
    if (b) {
        foo();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
