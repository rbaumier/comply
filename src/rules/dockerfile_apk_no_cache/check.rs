//! dockerfile-apk-no-cache tree-sitter backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] prefilter = ["apk add"] => |node, source, ctx, diagnostics|
    let mut shell_text: Option<&str> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "shell_command" {
            shell_text = std::str::from_utf8(&source[child.byte_range()]).ok();
            break;
        }
    }
    let Some(text) = shell_text else { return; };
    if !text.contains("apk add") { return; }
    if text.contains("--no-cache") { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Use `apk add --no-cache` to avoid leaving the apk index in the layer.".into(),
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
    fn flags_apk_add_without_no_cache() {
        assert_eq!(run("RUN apk add curl\n").len(), 1);
    }

    #[test]
    fn allows_apk_add_with_no_cache() {
        assert!(run("RUN apk add --no-cache curl\n").is_empty());
    }
}
