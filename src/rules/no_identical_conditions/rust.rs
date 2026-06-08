//! no-identical-conditions Rust backend.
//!
//! Flag duplicate conditions in if/else-if chains.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    // Only process the top-level if (not nested else-if branches).
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause"
    {
        return;
    }

    let mut conditions: Vec<(String, tree_sitter::Node)> = Vec::new();
    let mut current = Some(node);

    while let Some(if_node) = current {
        if if_node.kind() != "if_expression" {
            break;
        }

        if let Some(cond) = if_node.child_by_field_name("condition")
            && let Ok(cond_text) = cond.utf8_text(source)
        {
            for (prev_text, _) in &conditions {
                if prev_text == cond_text {
                    let pos = cond.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-identical-conditions".into(),
                        message: format!(
                            "Duplicate condition `{}` in if/else-if chain.",
                            cond_text
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
            conditions.push((cond_text.to_string(), cond));
        }

        // Follow the else clause to the next if_expression.
        current = None;
        if let Some(alt) = if_node.child_by_field_name("alternative") {
            let count = alt.named_child_count();
            for i in 0..count {
                if let Some(child) = alt.named_child(i)
                    && child.kind() == "if_expression"
                {
                    current = Some(child);
                    break;
                }
            }
        }
    }
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
    fn flags_duplicate_condition() {
        let src = r#"fn f() {
    if x > 0 {
        do_a();
    } else if x > 0 {
        do_b();
    }
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_conditions() {
        let src = r#"fn f() {
    if x > 0 {
        do_a();
    } else if x < 0 {
        do_b();
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
