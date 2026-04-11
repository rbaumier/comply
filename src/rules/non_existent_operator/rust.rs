//! non-existent-operator Rust backend.
//!
//! Detect `=+`, `=-`, `=!` typo operators. In Rust, `x =+ 1` parses as
//! `x = (+1)` — an assignment with a unary plus.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "assignment_expression" {
        return;
    }

    let Some(rhs) = node.child_by_field_name("right") else { return };
    if rhs.kind() != "unary_expression" {
        return;
    }

    let Some(unary_op) = rhs.child(0) else { return };
    let unary_text = unary_op.utf8_text(source).unwrap_or("");
    if unary_text != "-" && unary_text != "!" {
        return;
    }

    // Check adjacency: `=` and unary op must be adjacent.
    // Find the `=` operator node.
    let mut eq_node = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.utf8_text(source).unwrap_or("") == "=" {
            eq_node = Some(child);
            break;
        }
    }
    let Some(eq) = eq_node else { return };
    let eq_end = eq.end_byte();
    let unary_start = unary_op.start_byte();

    if eq_end != unary_start {
        return; // there's a space — intentional `x = -1`.
    }

    let pos = node.start_position();
    let suggested = match unary_text {
        "-" => "-=",
        "!" => "!=",
        _ => return,
    };
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "non-existent-operator".into(),
        message: format!("Typo: `={unary_text}` should be `{suggested}`."),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn allows_intentional_negative() {
        assert!(run_on("fn f() { let mut x = 0; x = -1; }").is_empty());
    }
}
