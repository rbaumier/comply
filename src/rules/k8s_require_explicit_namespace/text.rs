//! k8s-require-explicit-namespace tree-sitter backend (YAML AST).
//!
//! Namespaced resources must set `metadata.namespace` explicitly. Cluster-
//! scoped kinds (Namespace, ClusterRole, Node, PV, …) are exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

/// Kinds that are cluster-scoped (never namespaced).
const CLUSTER_SCOPED: &[&str] = &[
    "Namespace",
    "ClusterRole",
    "ClusterRoleBinding",
    "Node",
    "PersistentVolume",
    "StorageClass",
    "CustomResourceDefinition",
    "MutatingWebhookConfiguration",
    "ValidatingWebhookConfiguration",
    "APIService",
    "PriorityClass",
    "IngressClass",
    "RuntimeClass",
];

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if CLUSTER_SCOPED.contains(&kind.as_str()) {
        return;
    }
    if has_namespace(node, source) {
        return;
    }
    let anchor = y::find_pair(node, source, "kind").unwrap_or(node);
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &anchor,
        super::META.id,
        "Namespaced resource must set metadata.namespace explicitly.".into(),
        Severity::Warning,
    ));
}

fn has_namespace(manifest: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(metadata) = y::descend_mapping(manifest, source, &["metadata"]) else {
        return false;
    };
    let Some(pair) = y::find_pair(metadata, source, "namespace") else {
        return false;
    };
    y::pair_scalar_value(pair, source).is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_missing_namespace() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: app\nspec: {}";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_namespace_set() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: app\n  namespace: prod\nspec: {}";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_cluster_scoped() {
        let yaml =
            "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nmetadata:\n  name: c";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_namespace_kind() {
        let yaml = "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: prod";
        assert!(run(yaml).is_empty());
    }
}
