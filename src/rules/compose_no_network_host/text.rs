//! compose-no-network-host text backend.
//!
//! Matches a `block_mapping_pair` whose key is `network_mode` and whose
//! scalar value (after quote stripping) is `host`. Limited to files
//! that look like compose (filename contains `compose`, or source
//! contains a top-level `services:` key) so the same key on unrelated
//! YAML doesn't trip.

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
    if pair_key_text(node, source).as_deref() != Some("network_mode") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(value) = pair_scalar_value(node, source) else { return; };
    if value.trim() != "host" { return; }

    let value_node = node.named_child(1).unwrap_or(node);
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: 1,
        rule_id: super::META.id.into(),
        message: "`network_mode: host` bypasses Docker's network namespace; \
                  use a user-defined network with `ports:` instead.".into(),
        severity: Severity::Error,
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
    fn flags_network_mode_host_unquoted() {
        let src = "services:\n  api:\n    network_mode: host\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_network_mode_host_quoted() {
        let src = "services:\n  api:\n    network_mode: \"host\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_network_mode_bridge() {
        let src = "services:\n  api:\n    network_mode: bridge\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_network_mode() {
        let src = "services:\n  api:\n    image: foo:1.0\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_compose_yaml() {
        let src = "name: my-app\n";
        let run_non = |s: &str| run_yaml_with_path(s, &Check, "config.yml");
        assert!(run_non(src).is_empty());
    }
}
