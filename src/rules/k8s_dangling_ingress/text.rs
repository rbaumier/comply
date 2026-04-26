//! k8s-dangling-ingress tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::k8s_index::K8sIndex;
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Ingress" { return; }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let namespace = y::descend_mapping(node, source, &["metadata"])
        .and_then(|meta| y::find_pair(meta, source, "namespace"))
        .and_then(|pair| y::pair_scalar_value(pair, source))
        .unwrap_or_else(|| K8sIndex::default_namespace().to_string());

    let Some(rules_seq) = y::descend_sequence(node, source, &["spec", "rules"]) else { return; };
    for rule_map in y::sequence_item_mappings(rules_seq) {
        let Some(paths_seq) = y::descend_sequence(rule_map, source, &["http", "paths"]) else { continue; };
        for path_map in y::sequence_item_mappings(paths_seq) {
            let Some(service_map) = y::descend_mapping(path_map, source, &["backend", "service"]) else { continue; };
            let Some(name_pair) = y::find_pair(service_map, source, "name") else { continue; };
            let Some(name) = y::pair_scalar_value(name_pair, source) else { continue; };

            if !k8s_index.has_resource("Service", &namespace, &name) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &name_pair,
                    super::META.id,
                    format!(
                        "Ingress backend references Service/{name} in namespace {namespace}, but no such Service exists in the project."
                    ),
                    Severity::Warning,
                ));
            }
        }
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
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: Ingress\nmetadata:\n  name: web\nspec:\n  rules:\n  - http:\n      paths:\n      - path: /\n        pathType: Prefix\n        backend:\n          service:\n            name: web-svc\n            port:\n              number: 80";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_ingress_kinds() {
        let yaml =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  replicas: 1";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_missing_backend_service_in_project() {
        let ingress = "apiVersion: networking.k8s.io/v1\nkind: Ingress\nmetadata:\n  name: web\nspec:\n  rules:\n  - http:\n      paths:\n      - path: /\n        pathType: Prefix\n        backend:\n          service:\n            name: web-svc\n            port:\n              number: 80";
        let (_dir, project, paths) = k8s_project_from_sources(&[("ingress.yaml", ingress)]);
        let diags = run_yaml_with_project_and_path(ingress, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_existing_backend_service_in_project() {
        let ingress = "apiVersion: networking.k8s.io/v1\nkind: Ingress\nmetadata:\n  name: web\nspec:\n  rules:\n  - http:\n      paths:\n      - path: /\n        pathType: Prefix\n        backend:\n          service:\n            name: web-svc\n            port:\n              number: 80";
        let service = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web-svc\nspec:\n  selector:\n    app: web\n  ports:\n  - port: 80";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("ingress.yaml", ingress), ("svc.yaml", service)]);
        let diags = run_yaml_with_project_and_path(ingress, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
