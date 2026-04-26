//! k8s-no-privileged-container tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(sc) = y::descend_mapping(container, source, &["securityContext"]) else { continue; };
        let Some(pair) = y::find_pair(sc, source, "privileged") else { continue; };
        if y::pair_scalar_value(pair, source).as_deref() == Some("true") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &pair,
                super::META.id,
                "Container runs as privileged; this disables container isolation.".into(),
                Severity::Warning,
            ));
        }
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
    fn flags_privileged_true() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        securityContext:\n          privileged: true";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_privileged_false() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        securityContext:\n          privileged: false";
        assert!(run(yaml).is_empty());
    }
}
