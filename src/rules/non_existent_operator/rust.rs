//! non-existent-operator Rust backend.
//!
//! Detect `=+`, `=-`, `=!` typo operators. In Rust, `x =+ 1` parses as
//! `x = (+1)` — an assignment with a unary plus.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
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
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "non-existent-operator".into(),
        message: format!("Typo: `={unary_text}` should be `{suggested}`."),
        severity: Severity::Error,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn allows_intentional_negative() {
        assert!(run_on("fn f() { let mut x = 0; x = -1; }").is_empty());
    }
}
