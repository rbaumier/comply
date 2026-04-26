//! k8s-deployment-anti-affinity tree-sitter backend (YAML AST).
//!
//! Flags `Deployment` manifests with `spec.replicas > 1` whose pod template
//! lacks `spec.affinity.podAntiAffinity`. A missing `replicas` defaults to 1
//! and is out of scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Deployment" {
        return;
    }
    let Some(spec) = y::descend_mapping(node, source, &["spec"]) else { return; };
    let Some(replicas_pair) = y::find_pair(spec, source, "replicas") else { return; };
    let Some(value) = y::pair_scalar_value(replicas_pair, source) else { return; };
    let Ok(n) = value.parse::<i64>() else { return; };
    if n <= 1 {
        return;
    }
    let template_spec = y::descend_mapping(node, source, &["spec", "template", "spec"]);
    let has_anti_affinity = template_spec
        .and_then(|ts| y::descend_mapping(ts, source, &["affinity", "podAntiAffinity"]))
        .is_some();
    if !has_anti_affinity {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &replicas_pair,
            super::META.id,
            format!("Deployment with replicas: {n} must declare spec.template.spec.affinity.podAntiAffinity."),
            Severity::Warning,
        ));
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
    fn flags_missing_anti_affinity() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: 3\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_anti_affinity_present() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: 3\n  template:\n    spec:\n      affinity:\n        podAntiAffinity:\n          preferredDuringSchedulingIgnoredDuringExecution:\n          - weight: 100\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_single_replica() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: 1\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
