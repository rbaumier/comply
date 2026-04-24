//! k8s-require-liveness-probe tree-sitter backend (YAML AST).
//!
//! Walks document → top mapping → pod spec → containers[] and flags any
//! container that omits the `livenessProbe` key. Skipped for `Job` and
//! `CronJob` workloads, which are short-lived and don't need live probes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind == "Job" || kind == "CronJob" {
        return;
    }
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, false) {
        if !y::has_key(container, source, "livenessProbe") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &container,
                super::META.id,
                "Container must define a livenessProbe.".into(),
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
    fn flags_missing_liveness() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_liveness_set() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        livenessProbe:\n          httpGet:\n            path: /\n            port: 8080";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn skips_cronjob() {
        let yaml = "apiVersion: batch/v1\nkind: CronJob\nspec:\n  jobTemplate:\n    spec:\n      template:\n        spec:\n          containers:\n          - name: app\n            image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
