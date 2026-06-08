//! elseif-without-else Rust backend — flag `if/else if` chains without
//! a final `else`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression"] => |node, _source, ctx, diagnostics|
    // Only process top-level if expressions (not those inside else clauses).
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
    }

    // Walk the chain to find `else if` and check for final `else`.
    let mut has_else_if = false;
    let mut current: tree_sitter::Node = node;
    let mut last_else_if_node: tree_sitter::Node = node;

    loop {
        let Some(alt) = current.child_by_field_name("alternative") else {
            break;
        };
        if alt.kind() != "else_clause" {
            break;
        }

        // Check if the else_clause contains another if_expression.
        let mut found_nested_if = false;
        let child_count = alt.named_child_count();
        for i in 0..child_count {
            if let Some(child) = alt.named_child(i)
                && child.kind() == "if_expression" {
                    has_else_if = true;
                    last_else_if_node = child;
                    current = child;
                    found_nested_if = true;
                    break;
            }
        }
        if !found_nested_if {
            // Bare `else { ... }` — chain is complete.
            return;
        }
    }

    if !has_else_if {
        return;
    }

    let pos = last_else_if_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elseif-without-else".into(),
        message: "`if/else if` chain without a final `else` \
                  \u{2014} add an `else` block to handle remaining cases."
            .into(),
        severity: Severity::Warning,
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
    fn flags_else_if_without_else() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        do_a();
    } else if b {
        do_b();
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_else_if_with_else() {
        let src = r#"
fn f(a: bool, b: bool) {
    if a {
        do_a();
    } else if b {
        do_b();
    } else {
        do_c();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_if_without_else() {
        let src = r#"
fn f(a: bool) {
    if a {
        do_a();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
