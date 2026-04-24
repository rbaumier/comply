//! k8s-no-default-service-account tree-sitter backend (YAML AST).
//!
//! Pod-owning workloads must set `spec.template.spec.serviceAccountName`
//! (or bare Pod `spec.serviceAccountName`) to something other than the
//! default service account.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

const POD_OWNER_KINDS: &[&str] = &[
    "Pod",
    "Deployment",
    "StatefulSet",
    "DaemonSet",
    "Job",
    "CronJob",
    "ReplicaSet",
    "ReplicationController",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if !POD_OWNER_KINDS.contains(&kind.as_str()) {
        return;
    }
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let sa_pair = y::find_pair(pod_spec, source, "serviceAccountName");
    match sa_pair {
        None => {
            let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &kind_pair,
                super::META.id,
                "Pod spec must set serviceAccountName (do not rely on `default`).".into(),
                Severity::Warning,
            ));
        }
        Some(pair) => {
            if y::pair_scalar_value(pair, source).as_deref() == Some("default") {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &pair,
                    super::META.id,
                    "serviceAccountName must not be `default`; use a dedicated ServiceAccount.".into(),
                    Severity::Warning,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_missing_sa() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers: []";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_default_sa() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      serviceAccountName: default";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_custom_sa() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      serviceAccountName: my-app";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_pod_kinds() {
        let yaml = "apiVersion: v1\nkind: Service\nspec: {}";
        assert!(run(yaml).is_empty());
    }
}
