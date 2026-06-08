//! dockerfile-use-frozen-lockfile tree-sitter backend.
//!
//! `pnpm install` or `yarn install`/`yarn add` without `--frozen-lockfile`
//! (or `--immutable` for yarn berry) silently regenerates the lockfile
//! during the build, defeating reproducibility.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["run_instruction"] prefilter = ["yarn install", "yarn add", "pnpm install"] => |node, source, ctx, diagnostics|
    let shell_text = run_shell_text(node, source);
    let is_pnpm = shell_text.contains("pnpm install");
    let is_yarn = shell_text.contains("yarn install") || shell_text.contains("yarn add");
    if !(is_pnpm || is_yarn) {
        return;
    }
    if shell_text.contains("--frozen-lockfile") || shell_text.contains("--immutable") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "pnpm/yarn install must pass `--frozen-lockfile` (or `--immutable`) in Dockerfiles.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn run_shell_text<'a>(run: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    let mut cursor = run.walk();
    for c in run.children(&mut cursor) {
        if c.kind() == "shell_command" {
            return c.utf8_text(source).unwrap_or("");
        }
    }
    ""
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
    fn flags_pnpm_without_frozen() {
        assert_eq!(run("RUN pnpm install").len(), 1);
    }

    #[test]
    fn allows_pnpm_with_frozen() {
        assert!(run("RUN pnpm install --frozen-lockfile").is_empty());
    }

    #[test]
    fn allows_yarn_immutable() {
        assert!(run("RUN yarn install --immutable").is_empty());
    }
}
