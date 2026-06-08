//! dockerfile-use-cache-mount tree-sitter backend.
//!
//! Flags `RUN <package-manager-install>` that doesn't pass
//! `--mount=type=cache,...`. BuildKit cache mounts shave seconds-to-minutes
//! off rebuilds without leaking the cache into the final image layer.

use crate::diagnostic::{Diagnostic, Severity};

const PACKAGE_MANAGERS: &[&str] = &[
    "npm ci",
    "npm install",
    "pnpm install",
    "yarn install",
    "pip install",
    "apt install",
    "apt-get install",
];

crate::ast_check! { on ["run_instruction"] prefilter = ["npm ci", "npm install", "pnpm install", "yarn install", "pip install", "apt install", "apt-get install"] => |node, source, ctx, diagnostics|
    let full_text = node.utf8_text(source).unwrap_or("");
    let shell_text = run_shell_text(node, source);
    if !PACKAGE_MANAGERS.iter().any(|m| shell_text.contains(m)) {
        return;
    }
    if full_text.contains("--mount=type=cache") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "Package-manager RUN step should use `--mount=type=cache` for faster rebuilds.".into(),
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
    fn flags_npm_ci_without_cache() {
        assert_eq!(run("RUN npm ci\n").len(), 1);
    }

    #[test]
    fn allows_with_cache_mount() {
        let src = "RUN --mount=type=cache,target=/root/.npm npm ci\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_run() {
        assert!(run("RUN echo hi\n").is_empty());
    }
}
