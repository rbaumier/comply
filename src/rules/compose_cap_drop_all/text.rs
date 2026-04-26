//! compose-cap-drop-all text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{as_mapping, as_sequence, find_pair, pair_key_text, pair_value_node};

fn looks_like_compose(path: &std::path::Path, source: &str) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains("compose") {
        return true;
    }
    source.lines().any(|l| l == "services:" || l.starts_with("services:"))
}

/// `cap_drop: [ALL]` (flow sequence) or `cap_drop:\n  - ALL` (block sequence).
fn service_drops_all(service_map: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(cap_drop_pair) = find_pair(service_map, source, "cap_drop") else {
        return false;
    };
    let Some(value) = pair_value_node(cap_drop_pair) else {
        return false;
    };
    // Flow form `[ALL]` — value is a flow_node wrapping flow_sequence.
    if value.kind() == "flow_node" {
        let mut cursor = value.walk();
        for child in value.named_children(&mut cursor) {
            if child.kind() == "flow_sequence" {
                let mut icur = child.walk();
                return child.named_children(&mut icur).any(|item| {
                    item.utf8_text(source)
                        .ok()
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').eq_ignore_ascii_case("ALL"))
                        .unwrap_or(false)
                });
            }
        }
    }
    // Block form `- ALL`.
    if let Some(seq) = as_sequence(value) {
        let mut cursor = seq.walk();
        return seq.named_children(&mut cursor).any(|item| {
            if item.kind() != "block_sequence_item" {
                return false;
            }
            let mut icur = item.walk();
            item.named_children(&mut icur).any(|c| {
                c.utf8_text(source)
                    .ok()
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').eq_ignore_ascii_case("ALL"))
                    .unwrap_or(false)
            })
        });
    }
    false
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
        if service_drops_all(svc_map, source) { continue; }

        let key = service_pair.named_child(0).unwrap_or(service_pair);
        let pos = key.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!("Service `{name}` does not declare `cap_drop: [ALL]`."),
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
    fn flags_missing_cap_drop() {
        let src = "services:\n  api:\n    image: foo:1.0\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_flow_form() {
        let src = "services:\n  api:\n    image: foo:1.0\n    cap_drop: [ALL]\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_list_form() {
        let src = "services:\n  api:\n    image: foo:1.0\n    cap_drop:\n      - ALL\n";
        assert!(run(src).is_empty());
    }
}
