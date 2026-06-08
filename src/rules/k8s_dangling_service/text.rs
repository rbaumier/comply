//! k8s-dangling-service tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::k8s_index::K8sIndex;
use crate::rules::yaml_k8s_helpers as y;
use rustc_hash::FxHashMap;
use tree_sitter::Node;

fn collect_pairs(mapping: Node, source: &[u8]) -> FxHashMap<String, String> {
    let mut out = FxHashMap::default();
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

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Service" { return; }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let Some(selector) = y::descend_mapping(node, source, &["spec", "selector"])
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
                "Service selector does not match any workload's pod template labels in namespace {namespace}; the Service routes to nothing."
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
        // In single-file/test context, k8s_index is empty so the rule no-ops.
        let yaml = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\nspec:\n  selector:\n    app: web\n  ports:\n  - port: 80";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_service_kinds() {
        let yaml =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  replicas: 1";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_selector_without_matching_pods_in_project() {
        let service = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\nspec:\n  selector:\n    app: web\n  ports:\n  - port: 80";
        let (_dir, project, paths) = k8s_project_from_sources(&[("svc.yaml", service)]);
        let diags = run_yaml_with_project_and_path(service, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_selector_matching_workload_in_project() {
        let service = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\nspec:\n  selector:\n    app: web\n  ports:\n  - port: 80";
        let deployment = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    metadata:\n      labels:\n        app: web\n    spec:\n      containers:\n      - name: app\n        image: nginx";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("svc.yaml", service), ("deploy.yaml", deployment)]);
        let diags = run_yaml_with_project_and_path(service, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
