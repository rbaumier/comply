//! no-lonely-if Rust backend — flag `if` as the sole statement in an
//! `else` block: `else { if cond { } }` should be `else if cond { }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "if_expression" {
        return;
    }

    // Is this if_expression the sole child of a block that is inside
    // an else_clause?
    //
    // In tree-sitter-rust, `else { if b {} }` may parse as:
    //   else_clause -> block -> if_expression          (tail expression)
    //   else_clause -> block -> expression_statement -> if_expression
    let Some(parent) = node.parent() else { return };

    // Unwrap optional expression_statement wrapper.
    let block = if parent.kind() == "expression_statement" {
        parent.parent()
    } else if parent.kind() == "block" {
        Some(parent)
    } else {
        return;
    };
    let Some(block) = block else { return };
    if block.kind() != "block" {
        return;
    }

    // The block must contain exactly one named child.
    if block.named_child_count() != 1 {
        return;
    }

    // The block must be inside an else_clause.
    let Some(else_clause) = block.parent() else { return };
    if else_clause.kind() != "else_clause" {
        return;
    }

    // The else_clause must belong to an if_expression.
    let Some(outer_if) = else_clause.parent() else { return };
    if outer_if.kind() != "if_expression" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-lonely-if".into(),
        message: "Unexpected `if` as the only statement in an `else` block \
                  \u{2014} use `else if` instead."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_lonely_if_in_else() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        foo();
    } else {
        if b {
            bar();
        }
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-lonely-if");
    }

    #[test]
    fn allows_else_if() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        foo();
    } else if b {
        bar();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_with_multiple_statements() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        foo();
    } else {
        setup();
        if b {
            bar();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
