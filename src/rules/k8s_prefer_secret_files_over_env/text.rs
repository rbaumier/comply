//! k8s-prefer-secret-files-over-env tree-sitter backend (YAML AST).
//!
//! Flags container env entries that use `valueFrom.secretKeyRef`. Even
//! when the source is a Secret, env vars are visible to child processes
//! and `kubectl describe`; mounting as a file is preferred.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        let Some(env) = y::descend_sequence(container, source, &["env"]) else { continue; };
        for entry in y::sequence_item_mappings(env) {
            if y::descend_mapping(entry, source, &["valueFrom", "secretKeyRef"]).is_some() {
                let report_node = y::find_pair(entry, source, "name").unwrap_or(entry);
                let name = y::find_pair(entry, source, "name")
                    .and_then(|p| y::pair_scalar_value(p, source))
                    .unwrap_or_default();
                let label = if name.is_empty() {
                    "Env var sourced from secretKeyRef; mount the Secret as a file instead.".to_string()
                } else {
                    format!("Env var `{name}` sourced from secretKeyRef; mount the Secret as a file instead.")
                };
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &report_node,
                    super::META.id,
                    label,
                    Severity::Warning,
                ));
            }
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
    fn flags_secret_key_ref_env() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    env:\n    - name: DB_PASSWORD\n      valueFrom:\n        secretKeyRef:\n          name: db-creds\n          key: password";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_no_secret_env() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
