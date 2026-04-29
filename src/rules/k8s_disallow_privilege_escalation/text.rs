//! k8s-disallow-privilege-escalation tree-sitter backend (YAML AST).
//!
//! Flags any container whose `securityContext.allowPrivilegeEscalation` is
//! missing or not the scalar literal `false`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        if !has_escalation_false(container, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &container,
                super::META.id,
                "Container must set securityContext.allowPrivilegeEscalation: false.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn has_escalation_false(container: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(sc) = y::descend_mapping(container, source, &["securityContext"]) else {
        return false;
    };
    let Some(pair) = y::find_pair(sc, source, "allowPrivilegeEscalation") else {
        return false;
    };
    y::pair_scalar_value(pair, source).as_deref() == Some("false")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_missing_field() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_false_value() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        securityContext:\n          allowPrivilegeEscalation: false";
        assert!(run(yaml).is_empty());
    }
}
