//! no-duplicate-in-composite — flag duplicate types in union (`|`)
//! or intersection (`&`) type expressions.
//!
//! Matches `union_type` and `intersection_type` nodes in the
//! tree-sitter TypeScript grammar and checks for repeated members.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { on ["union_type", "intersection_type"] => |node, source, ctx, diagnostics|
match node.kind() {
        "union_type" | "intersection_type" => {}
        _ => return,
    }

    // Collect all named children (the type members)
    let count = node.named_child_count();
    if count < 2 {
        return;
    }

    let mut seen = HashSet::new();
    for i in 0..count {
        let Some(child) = node.named_child(i) else { continue };
        let Ok(text) = child.utf8_text(source) else { continue };
        let normalized = text.trim();
        if !normalized.is_empty() && !seen.insert(normalized.to_string()) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-duplicate-in-composite".into(),
                message: "Duplicate type in composite — remove the repeated member.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return; // one diagnostic per composite
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_duplicate_in_union() {
        assert_eq!(run_on("type X = string | string;").len(), 1);
    }

    #[test]
    fn flags_duplicate_in_intersection() {
        assert_eq!(run_on("type X = A & A;").len(), 1);
    }

    #[test]
    fn allows_unique_members() {
        assert!(run_on("type X = string | number;").is_empty());
    }

    #[test]
    fn allows_single_type() {
        assert!(run_on("type X = string;").is_empty());
    }
}
