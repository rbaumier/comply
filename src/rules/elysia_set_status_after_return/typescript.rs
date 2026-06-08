//! elysia-set-status-after-return backend — within a function/arrow body,
//! flag `set.status = ...` assignments that appear textually after a `return`
//! statement at the same nesting level.

use crate::diagnostic::{Diagnostic, Severity};

/// Scan immediate children (statement_block / function body): once a
/// `return_statement` is observed at this level, any later
/// `set.status = ...` expression statement is dead.
fn scan_block<'a>(
    block: tree_sitter::Node<'a>,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    out: &mut Vec<Diagnostic>,
) {
    let mut returned = false;
    let mut cursor = block.walk();
    for child in block.named_children(&mut cursor) {
        if child.kind() == "return_statement" {
            returned = true;
            continue;
        }
        if returned && child.kind() == "expression_statement" {
            let text = child.utf8_text(source).unwrap_or("");
            let trimmed = text.trim();
            if trimmed.starts_with("set.status") && trimmed.contains('=') {
                let pos = child.start_position();
                out.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "elysia-set-status-after-return".into(),
                    message: "`set.status = ...` after `return` has no effect — set the status before returning.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

crate::ast_check! { on ["statement_block"] prefilter = ["\"set.status\""] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    scan_block(node, source, ctx, diagnostics);
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_set_status_after_return() {
        let src = "import { Elysia } from 'elysia';\napp.get('/x', ({ set }) => {\n  return { ok: true };\n  set.status = 404;\n});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_set_status_before_return() {
        let src = "import { Elysia } from 'elysia';\napp.get('/x', ({ set }) => {\n  set.status = 404;\n  return { ok: true };\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_set_status_alone() {
        let src = "import { Elysia } from 'elysia';\napp.get('/x', ({ set }) => {\n  set.status = 200;\n});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "function h() { return 1; this.set.status = 404; }";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
