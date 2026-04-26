//! k8s-rbac-no-cluster-admin-binding tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "RoleBinding" && kind != "ClusterRoleBinding" { return; }
    let Some(role_ref) = y::descend_mapping(node, source, &["roleRef"]) else { return; };
    let Some(name_pair) = y::find_pair(role_ref, source, "name") else { return; };
    if y::pair_scalar_value(name_pair, source).as_deref() == Some("cluster-admin") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &name_pair,
            super::META.id,
            "Binding targets the `cluster-admin` role; use a least-privilege role instead.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_yaml(s, &Check)
    }

    #[test]
    fn flags_cluster_admin() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRoleBinding\nroleRef:\n  name: cluster-admin\n  kind: ClusterRole";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_other_role() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRoleBinding\nroleRef:\n  name: my-role\n  kind: ClusterRole";
        assert!(run(yaml).is_empty());
    }
}
