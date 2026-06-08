//! dockerfile-require-multi-stage tree-sitter backend.
//!
//! Single-stage Dockerfiles ship build tools to production. Flags Dockerfiles
//! that have exactly one `FROM` instruction with no `AS` alias.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut from_nodes: Vec<tree_sitter::Node> = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "from_instruction" {
            from_nodes.push(child);
        }
    }
    if from_nodes.is_empty() {
        return;
    }
    let any_aliased = from_nodes.iter().any(|f| {
        let mut c = f.walk();
        f.children(&mut c).any(|n| n.kind() == "image_alias")
    });
    if from_nodes.len() >= 2 || any_aliased {
        return;
    }
    let pos = from_nodes[0].start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "Single-stage Dockerfile — use `FROM ... AS build` plus a runtime stage to keep the final image minimal.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "Dockerfile")
    }

    #[test]
    fn flags_single_stage() {
        assert_eq!(run("FROM node:22.12\nRUN npm ci\n").len(), 1);
    }

    #[test]
    fn allows_explicit_as() {
        assert!(run("FROM node:22.12 AS build\nRUN npm ci\n").is_empty());
    }

    #[test]
    fn allows_two_stages() {
        let src = "FROM node:22.12 AS build\nRUN npm ci\nFROM nginx:1.27.3\n";
        assert!(run(src).is_empty());
    }
}
