//! k8s-rolling-update-zero-unavailable tree-sitter backend (YAML AST).
//!
//! Deployment rollouts default to `maxUnavailable: 25%`, which can drop
//! requests. Walk each Deployment manifest and ensure
//! `spec.strategy.rollingUpdate.maxUnavailable == 0`. If the key is missing
//! we still flag (the default is non-zero); if present with any other value
//! we flag with that same message anchored at the value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Deployment" {
        return;
    }
    let pair = y::descend_mapping(node, source, &["spec", "strategy", "rollingUpdate"])
        .and_then(|m| y::find_pair(m, source, "maxUnavailable"));
    match pair {
        Some(pair_node) => {
            if y::pair_scalar_value(pair_node, source).as_deref() != Some("0") {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &pair_node,
                    super::META.id,
                    "strategy.rollingUpdate.maxUnavailable must be 0 to avoid downtime.".into(),
                    Severity::Warning,
                ));
            }
        }
        None => {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Deployment must set strategy.rollingUpdate.maxUnavailable: 0 (default is 25%).".into(),
                Severity::Warning,
            ));
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
    fn flags_missing_max_unavailable() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: 3";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_nonzero_max_unavailable() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  strategy:\n    rollingUpdate:\n      maxUnavailable: 1";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_zero_max_unavailable() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  strategy:\n    rollingUpdate:\n      maxUnavailable: 0\n      maxSurge: 1";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_deployment() {
        let yaml = "apiVersion: v1\nkind: Service\nspec: {}";
        assert!(run(yaml).is_empty());
    }
}
