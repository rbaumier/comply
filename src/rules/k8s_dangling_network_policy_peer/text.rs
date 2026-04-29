//! k8s-dangling-network-policy-peer tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::k8s_index::K8sIndex;
use crate::rules::yaml_k8s_helpers as y;
use std::collections::HashMap;
use tree_sitter::Node;

fn collect_pairs(mapping: Node, source: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut cursor = mapping.walk();
    for child in mapping.named_children(&mut cursor) {
        if child.kind() != "block_mapping_pair" {
            continue;
        }
        let Some(key) = y::pair_key_text(child, source) else {
            continue;
        };
        let Some(val) = y::pair_scalar_value(child, source) else {
            continue;
        };
        out.insert(key, val);
    }
    out
}

fn check_peer_sequence(
    rule_items: Node,
    source: &[u8],
    peer_key: &str,
    namespace: &str,
    k8s_index: &K8sIndex,
    path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for rule_map in y::sequence_item_mappings(rule_items) {
        let Some(peers) = y::descend_sequence(rule_map, source, &[peer_key]) else {
            continue;
        };
        for peer_map in y::sequence_item_mappings(peers) {
            let Some(pod_selector) = y::descend_mapping(peer_map, source, &["podSelector"]) else {
                continue;
            };
            let Some(match_labels) = y::descend_mapping(pod_selector, source, &["matchLabels"])
            else {
                continue;
            };
            let labels = collect_pairs(match_labels, source);
            if labels.is_empty() {
                continue;
            }
            if !k8s_index.has_pods_matching(namespace, &labels) {
                diagnostics.push(Diagnostic::at_node(
                    path,
                    &pod_selector,
                    super::META.id,
                    format!(
                        "NetworkPolicy peer podSelector does not match any workload's pod template labels in namespace {namespace}; the rule references no pods."
                    ),
                    Severity::Warning,
                ));
            }
        }
    }
}

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "NetworkPolicy" { return; }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let namespace = y::descend_mapping(node, source, &["metadata"])
        .and_then(|meta| y::find_pair(meta, source, "namespace"))
        .and_then(|pair| y::pair_scalar_value(pair, source))
        .unwrap_or_else(|| K8sIndex::default_namespace().to_string());

    if let Some(ingress) = y::descend_sequence(node, source, &["spec", "ingress"]) {
        check_peer_sequence(ingress, source, "from", &namespace, k8s_index, ctx.path, diagnostics);
    }
    if let Some(egress) = y::descend_sequence(node, source, &["spec", "egress"]) {
        check_peer_sequence(egress, source, "to", &namespace, k8s_index, ctx.path, diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{
        k8s_project_from_sources, run_yaml, run_yaml_with_project_and_path,
    };

    fn run(s: &str) -> Vec<Diagnostic> {
        run_yaml(s, &Check)
    }

    #[test]
    fn skips_when_index_empty_single_file() {
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: NetworkPolicy\nmetadata:\n  name: allow\nspec:\n  podSelector: {}\n  ingress:\n  - from:\n    - podSelector:\n        matchLabels:\n          app: web";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_network_policy_kinds() {
        let yaml =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  replicas: 1";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_peers_without_pod_selector() {
        // ipBlock / namespaceSelector only — should not trigger lookups even with empty index.
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: NetworkPolicy\nmetadata:\n  name: allow\nspec:\n  podSelector: {}\n  egress:\n  - to:\n    - ipBlock:\n        cidr: 10.0.0.0/8";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_peer_selector_without_matching_pods_in_project() {
        let policy = "apiVersion: networking.k8s.io/v1\nkind: NetworkPolicy\nmetadata:\n  name: allow\nspec:\n  podSelector: {}\n  ingress:\n  - from:\n    - podSelector:\n        matchLabels:\n          app: web";
        let (_dir, project, paths) = k8s_project_from_sources(&[("policy.yaml", policy)]);
        let diags = run_yaml_with_project_and_path(policy, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_peer_selector_matching_workload_in_project() {
        let policy = "apiVersion: networking.k8s.io/v1\nkind: NetworkPolicy\nmetadata:\n  name: allow\nspec:\n  podSelector: {}\n  ingress:\n  - from:\n    - podSelector:\n        matchLabels:\n          app: web";
        let deployment = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    metadata:\n      labels:\n        app: web\n    spec:\n      containers:\n      - name: app\n        image: nginx";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("policy.yaml", policy), ("deploy.yaml", deployment)]);
        let diags = run_yaml_with_project_and_path(policy, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
