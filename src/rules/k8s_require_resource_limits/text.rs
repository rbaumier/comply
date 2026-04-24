//! k8s-require-resource-limits tree-sitter backend (YAML AST).
//!
//! A container satisfies the rule when `resources.limits.cpu` and
//! `resources.limits.memory` are both present with non-empty scalar values.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        if !has_limit_scalar(container, source, "cpu")
            || !has_limit_scalar(container, source, "memory")
        {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &container,
                super::META.id,
                "Container must declare resources.limits.cpu and resources.limits.memory.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn has_limit_scalar(container: tree_sitter::Node, source: &[u8], field: &str) -> bool {
    let Some(limits) = y::descend_mapping(container, source, &["resources", "limits"]) else {
        return false;
    };
    let Some(pair) = y::find_pair(limits, source, field) else {
        return false;
    };
    y::pair_scalar_value(pair, source)
        .is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_missing_limits() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_limits_set() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        resources:\n          limits:\n            cpu: 500m\n            memory: 256Mi";
        assert!(run(yaml).is_empty());
    }
}
