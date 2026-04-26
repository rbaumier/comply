//! k8s-dangling-service-monitor tree-sitter backend (YAML AST).

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "ServiceMonitor" { return; }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let Some(selector) = y::descend_mapping(node, source, &["spec", "selector", "matchLabels"])
    else { return; };

    let selector_map = collect_pairs(selector, source);
    if selector_map.is_empty() { return; }

    let namespace = y::descend_mapping(node, source, &["metadata"])
        .and_then(|meta| y::find_pair(meta, source, "namespace"))
        .and_then(|pair| y::pair_scalar_value(pair, source))
        .unwrap_or_else(|| K8sIndex::default_namespace().to_string());

    if !k8s_index.has_pods_matching(&namespace, &selector_map) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &selector,
            super::META.id,
            format!(
                "ServiceMonitor selector does not match any workload's pod template labels in namespace {namespace}; the monitor scrapes nothing."
            ),
            Severity::Warning,
        ));
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
        let yaml = "apiVersion: monitoring.coreos.com/v1\nkind: ServiceMonitor\nmetadata:\n  name: web\nspec:\n  selector:\n    matchLabels:\n      app: web\n  endpoints:\n  - port: metrics\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_servicemonitor_kinds() {
        let yaml =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  replicas: 1\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_selector_without_matching_pods_in_project() {
        let monitor = "apiVersion: monitoring.coreos.com/v1\nkind: ServiceMonitor\nmetadata:\n  name: web\nspec:\n  selector:\n    matchLabels:\n      app: web\n  endpoints:\n  - port: metrics\n";
        let (_dir, project, paths) = k8s_project_from_sources(&[("monitor.yaml", monitor)]);
        let diags = run_yaml_with_project_and_path(monitor, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_selector_matching_workload_in_project() {
        let monitor = "apiVersion: monitoring.coreos.com/v1\nkind: ServiceMonitor\nmetadata:\n  name: web\nspec:\n  selector:\n    matchLabels:\n      app: web\n  endpoints:\n  - port: metrics\n";
        let deployment = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    metadata:\n      labels:\n        app: web\n    spec:\n      containers:\n      - name: app\n        image: nginx";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("monitor.yaml", monitor), ("deploy.yaml", deployment)]);
        let diags = run_yaml_with_project_and_path(monitor, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
