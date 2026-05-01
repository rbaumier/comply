//! k8s-hpa-min-three-replicas tree-sitter backend (YAML AST).
//!
//! Flags `HorizontalPodAutoscaler` manifests whose `spec.minReplicas` is
//! a number strictly less than 3.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "HorizontalPodAutoscaler" {
        return;
    }
    let Some(spec) = y::descend_mapping(node, source, &["spec"]) else { return; };
    let Some(pair) = y::find_pair(spec, source, "minReplicas") else { return; };
    let Some(value) = y::pair_scalar_value(pair, source) else { return; };
    let min_replicas = ctx.config.threshold("k8s-hpa-min-three-replicas", "min_replicas", ctx.lang);
    if let Ok(n) = value.parse::<i64>()
        && n < min_replicas as i64
    {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &pair,
            super::META.id,
            format!("HPA minReplicas must be >= {min_replicas} (found {n})."),
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
    fn flags_min_replicas_one() {
        let yaml =
            "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\nspec:\n  minReplicas: 1";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_min_replicas_three() {
        let yaml =
            "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\nspec:\n  minReplicas: 3";
        assert!(run(yaml).is_empty());
    }
}
