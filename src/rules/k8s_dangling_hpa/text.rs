//! k8s-dangling-hpa tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::k8s_index::K8sIndex;
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "HorizontalPodAutoscaler" { return; }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let Some(scale_target_ref) =
        y::descend_mapping(node, source, &["spec", "scaleTargetRef"])
    else { return; };

    let Some(target_kind_pair) = y::find_pair(scale_target_ref, source, "kind") else { return; };
    let Some(target_kind) = y::pair_scalar_value(target_kind_pair, source) else { return; };
    let Some(target_name_pair) = y::find_pair(scale_target_ref, source, "name") else { return; };
    let Some(target_name) = y::pair_scalar_value(target_name_pair, source) else { return; };

    let namespace = y::descend_mapping(node, source, &["metadata"])
        .and_then(|meta| y::find_pair(meta, source, "namespace"))
        .and_then(|pair| y::pair_scalar_value(pair, source))
        .unwrap_or_else(|| K8sIndex::default_namespace().to_string());

    if !k8s_index.has_resource(&target_kind, &namespace, &target_name) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &scale_target_ref,
            super::META.id,
            format!(
                "HPA scaleTargetRef points to {target_kind}/{target_name} in namespace {namespace}, but no such resource exists in the project."
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
        let yaml = "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\nmetadata:\n  name: web-hpa\nspec:\n  scaleTargetRef:\n    apiVersion: apps/v1\n    kind: Deployment\n    name: web\n  minReplicas: 2\n  maxReplicas: 5";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_hpa_kinds() {
        let yaml =
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  replicas: 1";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_missing_scale_target_in_project() {
        let hpa = "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\nmetadata:\n  name: web-hpa\nspec:\n  scaleTargetRef:\n    apiVersion: apps/v1\n    kind: Deployment\n    name: web\n  minReplicas: 2\n  maxReplicas: 5";
        let (_dir, project, paths) = k8s_project_from_sources(&[("hpa.yaml", hpa)]);
        let diags = run_yaml_with_project_and_path(hpa, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_existing_scale_target_in_project() {
        let hpa = "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\nmetadata:\n  name: web-hpa\nspec:\n  scaleTargetRef:\n    apiVersion: apps/v1\n    kind: Deployment\n    name: web\n  minReplicas: 2\n  maxReplicas: 5";
        let deployment = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("hpa.yaml", hpa), ("deploy.yaml", deployment)]);
        let diags = run_yaml_with_project_and_path(hpa, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
