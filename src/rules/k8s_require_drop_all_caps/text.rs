//! k8s-require-drop-all-caps tree-sitter backend (YAML AST).
//!
//! Each container must include `ALL` in
//! `securityContext.capabilities.drop` (either block list or flow list).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        if !drop_includes_all(container, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &container,
                super::META.id,
                "Container must drop ALL capabilities via securityContext.capabilities.drop.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn drop_includes_all(container: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(drop_pair) = find_drop_pair(container, source) else {
        return false;
    };
    let Some(value) = y::pair_value_node(drop_pair) else {
        return false;
    };
    sequence_contains(value, source, "ALL")
}

fn find_drop_pair<'t>(
    container: tree_sitter::Node<'t>,
    source: &[u8],
) -> Option<tree_sitter::Node<'t>> {
    let caps = y::descend_mapping(container, source, &["securityContext", "capabilities"])?;
    y::find_pair(caps, source, "drop")
}

fn sequence_contains(value: tree_sitter::Node, source: &[u8], needle: &str) -> bool {
    match value.kind() {
        "block_node" | "flow_node" => {
            let mut cursor = value.walk();
            value
                .named_children(&mut cursor)
                .any(|c| sequence_contains(c, source, needle))
        }
        "block_sequence" => {
            let mut cursor = value.walk();
            for item in value.named_children(&mut cursor) {
                if item.kind() != "block_sequence_item" {
                    continue;
                }
                let mut icur = item.walk();
                for ichild in item.named_children(&mut icur) {
                    if scalar_equals(ichild, source, needle) {
                        return true;
                    }
                }
            }
            false
        }
        "flow_sequence" => {
            let mut cursor = value.walk();
            value
                .named_children(&mut cursor)
                .any(|c| scalar_equals(c, source, needle))
        }
        _ => scalar_equals(value, source, needle),
    }
}

fn scalar_equals(node: tree_sitter::Node, source: &[u8], needle: &str) -> bool {
    node.utf8_text(source)
        .ok()
        .map(|s| s.trim().trim_matches('"').trim_matches('\'') == needle)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_missing_drop_all() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_block_style_all() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        securityContext:\n          capabilities:\n            drop:\n              - ALL";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn allows_flow_style_all() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        securityContext:\n          capabilities:\n            drop: [\"ALL\"]";
        assert!(run(yaml).is_empty());
    }
}
