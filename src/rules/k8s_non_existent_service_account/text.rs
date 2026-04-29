//! k8s-non-existent-service-account tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::k8s_index::K8sIndex;
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if !matches!(
        kind.as_str(),
        "Pod" | "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" | "CronJob"
    ) {
        return;
    }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let Some(sa_pair) = y::find_pair(pod_spec, source, "serviceAccountName") else { return; };
    let Some(sa_name) = y::pair_scalar_value(sa_pair, source) else { return; };

    let namespace = y::descend_mapping(node, source, &["metadata"])
        .and_then(|meta| y::find_pair(meta, source, "namespace"))
        .and_then(|pair| y::pair_scalar_value(pair, source))
        .unwrap_or_else(|| K8sIndex::default_namespace().to_string());

    if !k8s_index.has_resource("ServiceAccount", &namespace, &sa_name) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &sa_pair,
            super::META.id,
            format!(
                "{kind} references ServiceAccount/{sa_name} in namespace {namespace}, but no such ServiceAccount exists in the project."
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
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    spec:\n      serviceAccountName: missing-sa\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_kinds_without_pod_spec() {
        let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cfg\ndata:\n  key: value";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_missing_service_account_in_project() {
        let deploy = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    spec:\n      serviceAccountName: web-sa\n      containers:\n      - name: app\n        image: nginx";
        let (_dir, project, paths) = k8s_project_from_sources(&[("deploy.yaml", deploy)]);
        let diags = run_yaml_with_project_and_path(deploy, &Check, &project, &paths[0]);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_existing_service_account_in_project() {
        let deploy = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\nspec:\n  template:\n    spec:\n      serviceAccountName: web-sa\n      containers:\n      - name: app\n        image: nginx";
        let service_account = "apiVersion: v1\nkind: ServiceAccount\nmetadata:\n  name: web-sa";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("deploy.yaml", deploy), ("sa.yaml", service_account)]);
        let diags = run_yaml_with_project_and_path(deploy, &Check, &project, &paths[0]);
        assert!(diags.is_empty());
    }
}
