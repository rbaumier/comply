//! dockerfile-require-healthcheck tree-sitter backend.
//!
//! Flags Dockerfiles that contain at least one `FROM` instruction but no
//! `HEALTHCHECK` — orchestrators (Docker Swarm, Kubernetes liveness probes
//! that fall back to it, Compose) cannot detect a stuck container without one.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut saw_from = false;
    let mut saw_healthcheck = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "from_instruction" => saw_from = true,
            "healthcheck_instruction" => saw_healthcheck = true,
            _ => {}
        }
    }
    if saw_from && !saw_healthcheck {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Dockerfile missing HEALTHCHECK — orchestrators can't detect stuck containers.".into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "Dockerfile")
    }

    #[test]
    fn flags_missing_healthcheck() {
        assert_eq!(run("FROM node:22.12\nCMD [\"node\"]\n").len(), 1);
    }

    #[test]
    fn allows_healthcheck_present() {
        let src = "FROM node:22.12\nHEALTHCHECK CMD curl -f http://localhost/ || exit 1\nCMD [\"node\"]\n";
        assert!(run(src).is_empty());
    }
}
