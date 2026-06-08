//! dockerfile-add-for-archive-extract tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["add_instruction"] => |node, source, ctx, diagnostics|
    let mut first_path: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "path" {
            first_path = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(src) = first_path else { return; };
    let trimmed = src.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use RUN curl/wget + tar instead of ADD <url> to avoid leaving the archive in the layer.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
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
    fn flags_add_url() {
        assert_eq!(run("ADD https://example.com/app.tar.gz /app/\n").len(), 1);
    }

    #[test]
    fn allows_add_local_archive() {
        assert!(run("ADD app.tar.gz /app/\n").is_empty());
    }
}
