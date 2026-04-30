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
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_yaml_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml_with_path(source, &Check, "docker-compose.yml")
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
        let run_non = |s: &str| run_yaml_with_path(s, &Check, "config.yml");
        assert!(run_non(src).is_empty());
    }
}
