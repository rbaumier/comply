//! compose-healthcheck-required text backend.
//!
//! Walks every service mapping under the top-level `services:` block
//! and flags those that don't declare a `healthcheck:` key. We do NOT
//! recurse into the `healthcheck:` mapping itself — the presence of the
//! key is enough; whether the command is sensible is out of scope.
//!
//! Same shape as `compose-require-resource-limits`: anchored on the
//! `services:` `block_mapping_pair`, then a single pass over each
//! service to check for the required key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{as_mapping, find_pair, pair_key_text, pair_value_node};

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
    if pair_key_text(node, source).as_deref() != Some("services") { return; }
    if !looks_like_compose(ctx.path, ctx.source) { return; }

    let Some(services_value) = pair_value_node(node) else { return; };
    let Some(services_map) = as_mapping(services_value) else { return; };

    let mut cursor = services_map.walk();
    for service_pair in services_map.named_children(&mut cursor) {
        if service_pair.kind() != "block_mapping_pair" { continue; }
        let Some(name) = pair_key_text(service_pair, source) else { continue; };
        let Some(svc_value) = pair_value_node(service_pair) else { continue; };
        let Some(svc_map) = as_mapping(svc_value) else { continue; };
        if find_pair(svc_map, source, "healthcheck").is_some() { continue; }

        let key = service_pair.named_child(0).unwrap_or(service_pair);
        let pos = key.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Service `{name}` does not declare a `healthcheck:`; \
                 orchestrators can't tell whether it's actually serving."
            ),
            severity: Severity::Warning,
            span: Some((key.byte_range().start, key.byte_range().len())),
        });
    }
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
    fn flags_service_without_healthcheck() {
        let src = "services:\n  api:\n    image: foo:1.0\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_service_with_healthcheck() {
        let src = "services:\n  api:\n    image: foo:1.0\n    healthcheck:\n      test: [\"CMD\", \"curl\", \"-f\", \"http://localhost/health\"]\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_each_service_independently() {
        let src = "services:\n  api:\n    image: foo:1.0\n  worker:\n    image: bar:1.0\n    healthcheck:\n      test: [\"CMD\", \"true\"]\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_compose_yaml() {
        let src = "name: my-app\n";
        let run_non = |s: &str| run_yaml_with_path(s, &Check, "config.yml");
        assert!(run_non(src).is_empty());
    }
}
