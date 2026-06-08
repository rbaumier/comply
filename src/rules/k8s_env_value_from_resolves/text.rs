//! k8s-env-value-from-resolves tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::k8s_index::K8sIndex;
use crate::rules::yaml_k8s_helpers as y;

const WORKLOAD_KINDS: &[&str] = &[
    "Pod",
    "Deployment",
    "StatefulSet",
    "DaemonSet",
    "ReplicaSet",
    "Job",
    "CronJob",
];

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if !WORKLOAD_KINDS.contains(&kind.as_str()) { return; }

    let k8s_index = ctx.project.k8s_index();
    if k8s_index.is_empty() { return; }

    let namespace = y::descend_mapping(node, source, &["metadata"])
        .and_then(|meta| y::find_pair(meta, source, "namespace"))
        .and_then(|pair| y::pair_scalar_value(pair, source))
        .unwrap_or_else(|| K8sIndex::default_namespace().to_string());

    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };

    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(env_seq) = y::descend_sequence(container, source, &["env"]) else { continue; };
        for env_item in y::sequence_item_mappings(env_seq) {
            let Some(value_from) = y::descend_mapping(env_item, source, &["valueFrom"]) else { continue; };

            for (ref_key, resource_kind) in [
                ("secretKeyRef", "Secret"),
                ("configMapKeyRef", "ConfigMap"),
            ] {
                let Some(ref_node) = y::descend_mapping(value_from, source, &[ref_key]) else { continue; };
                let Some(name_pair) = y::find_pair(ref_node, source, "name") else { continue; };
                let Some(name) = y::pair_scalar_value(name_pair, source) else { continue; };

                if !k8s_index.has_resource(resource_kind, &namespace, &name) {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &ref_node,
                        super::META.id,
                        format!(
                            "{ref_key} references {resource_kind} \"{name}\" which does not exist in namespace {namespace}."
                        ),
                        Severity::Warning,
                    ));
                }
            }
        }
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{k8s_project_from_sources};

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "manifest.yaml")
    }

    #[test]
    fn skips_when_index_empty_single_file() {
        let yaml = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: web\nspec:\n  containers:\n  - name: app\n    image: nginx\n    env:\n    - name: DB_PASSWORD\n      valueFrom:\n        secretKeyRef:\n          name: db-secret\n          key: password\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_workload_kinds() {
        let yaml = "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\nspec:\n  selector:\n    app: web\n  ports:\n  - port: 80\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_missing_secret_ref_in_project() {
        let pod = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: web\nspec:\n  containers:\n  - name: app\n    image: nginx\n    env:\n    - name: DB_PASSWORD\n      valueFrom:\n        secretKeyRef:\n          name: db-secret\n          key: password\n";
        let (_dir, project, paths) = k8s_project_from_sources(&[("pod.yaml", pod)]);
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, pod, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_existing_secret_ref_in_project() {
        let pod = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: web\nspec:\n  containers:\n  - name: app\n    image: nginx\n    env:\n    - name: DB_PASSWORD\n      valueFrom:\n        secretKeyRef:\n          name: db-secret\n          key: password\n";
        let secret = "apiVersion: v1\nkind: Secret\nmetadata:\n  name: db-secret";
        let (_dir, project, paths) =
            k8s_project_from_sources(&[("pod.yaml", pod), ("secret.yaml", secret)]);
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, pod, &paths[0], &project, crate::rules::file_ctx::default_static_file_ctx());
        assert!(diags.is_empty());
    }
}
