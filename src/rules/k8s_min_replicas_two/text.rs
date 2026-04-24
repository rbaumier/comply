//! k8s-min-replicas-two tree-sitter backend (YAML AST).
//!
//! - `Deployment` must set `spec.replicas >= 2` (missing defaults to 1).
//! - `HorizontalPodAutoscaler` must set `spec.minReplicas >= 2`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let field = match kind.as_str() {
        "Deployment" => "replicas",
        "HorizontalPodAutoscaler" => "minReplicas",
        _ => return,
    };
    let spec = y::descend_mapping(node, source, &["spec"]);
    let replicas_pair = spec.and_then(|s| y::find_pair(s, source, field));
    match replicas_pair {
        Some(pair) => {
            let value = y::pair_scalar_value(pair, source).unwrap_or_default();
            if let Ok(n) = value.parse::<i64>()
                && n < 2 {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &pair,
                        super::META.id,
                        format!("{field} must be >= 2 for availability (found {n})."),
                        Severity::Warning,
                    ));
                }
        }
        None => {
            // Deployment without explicit replicas defaults to 1 — flag on `kind:`.
            let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &kind_pair,
                super::META.id,
                format!("{field} not set; defaults to 1. Set it to >= 2."),
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
    fn flags_replicas_one() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: 1";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_missing_replicas() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  selector: {}";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_replicas_two() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: 2";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn flags_hpa_min_one() {
        let yaml = "apiVersion: autoscaling/v2\nkind: HorizontalPodAutoscaler\nspec:\n  minReplicas: 1\n  maxReplicas: 5";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn ignores_other_kinds() {
        let yaml = "apiVersion: v1\nkind: Service\nspec: {}";
        assert!(run(yaml).is_empty());
    }
}
