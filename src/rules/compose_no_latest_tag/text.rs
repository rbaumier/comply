//! compose-no-latest-tag text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

fn looks_like_compose(path: &std::path::Path, source: &str) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("compose") {
        return true;
    }
    source
        .lines()
        .any(|l| l == "services:" || l.starts_with("services:"))
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("image") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(value) = pair_scalar_value(node, source) else { return; };
    if value.is_empty() { return; }
    if value.contains('@') { return; } // digest pin

    let has_tag = value.rsplit('/').next().is_some_and(|s| s.contains(':'));
    if has_tag && !value.ends_with(":latest") { return; }

    let value_node = node.named_child(1).unwrap_or(node);
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "Compose `image:` uses `:latest` or no tag; pin a precise version.".into(),
        severity: Severity::Warning,
        span: Some((value_node.byte_range().start, value_node.byte_range().len())),
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
    use crate::diagnostic::Diagnostic;
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "docker-compose.yml")
    }

    #[test]
    fn flags_latest_tag() {
        let src = "services:\n  db:\n    image: postgres:latest\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_missing_tag() {
        let src = "services:\n  db:\n    image: postgres\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pinned_tag() {
        let src = "services:\n  db:\n    image: postgres:16.6-alpine3.20\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_compose_yaml() {
        let src = "name: my-app\nversion: 1.0\n";
        let run_non = |s: &str| crate::rules::test_helpers::run_rule(&Check, s, "config.yml");
        assert!(run_non(src).is_empty());
    }
}
