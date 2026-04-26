//! k8s-restart-policy-required tree-sitter backend (YAML AST).
//!
//! Standalone `kind: Pod` manifests must declare `spec.restartPolicy`.
//! Pod templates inside higher-level workloads are out of scope; their
//! controllers manage restart semantics differently.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Pod" {
        return;
    }
    let Some(spec) = y::descend_mapping(node, source, &["spec"]) else { return; };
    if y::find_pair(spec, source, "restartPolicy").is_none() {
        let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &kind_pair,
            super::META.id,
            "Pod must set spec.restartPolicy explicitly (Always | OnFailure | Never).".into(),
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
    fn flags_missing_restart_policy() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_explicit_restart_policy() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  restartPolicy: Always\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_deployments() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
