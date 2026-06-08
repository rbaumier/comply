//! dockerfile-copy-trailing-slash tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["copy_instruction"] => |node, source, ctx, diagnostics|
    let mut paths: Vec<tree_sitter::Node> = Vec::new();
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "path" {
            paths.push(child);
        }
    }
    if paths.len() <= 2 { return; }
    let last = paths.last().unwrap();
    let Ok(text) = std::str::from_utf8(&source[last.byte_range()]) else { return; };
    let trimmed = text.trim_end();
    if trimmed.ends_with('/') { return; }
    let pos = last.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "COPY destination must end with `/` when copying multiple sources.".into(),
        severity: Severity::Warning,
        span: Some((last.byte_range().start, last.byte_range().len())),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "Dockerfile")
    }

    #[test]
    fn flags_missing_slash_with_multiple_sources() {
        assert_eq!(run("COPY file1.txt file2.txt /app\n").len(), 1);
    }

    #[test]
    fn allows_trailing_slash_with_multiple_sources() {
        assert!(run("COPY file1.txt file2.txt /app/\n").is_empty());
    }

    #[test]
    fn allows_single_source() {
        assert!(run("COPY file1.txt /app\n").is_empty());
    }
}
