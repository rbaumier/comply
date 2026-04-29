//! k8s-no-secret-in-env-literal tree-sitter backend (YAML AST).
//!
//! Flags env entries whose `name` looks secret-y (PASSWORD/TOKEN/SECRET/
//! API_KEY/APIKEY) AND whose value is provided as a literal `value:`
//! rather than `valueFrom:`.

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
            let Some(name_pair) = y::find_pair(entry, source, "name") else { continue; };
            let Some(name) = y::pair_scalar_value(name_pair, source) else { continue; };
            if !looks_sensitive(&name) {
                continue;
            }
            if y::find_pair(entry, source, "value").is_some()
                && y::find_pair(entry, source, "valueFrom").is_none()
            {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &name_pair,
                    super::META.id,
                    format!("Env var `{name}` looks like a secret; use valueFrom.secretKeyRef instead of a literal value."),
                    Severity::Warning,
                ));
            }
        }
    }
}

fn looks_sensitive(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.contains("PASSWORD")
        || upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("API_KEY")
        || upper.contains("APIKEY")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_password_literal() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    env:\n    - name: DB_PASSWORD\n      value: hunter2";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_value_from_secret() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    env:\n    - name: DB_PASSWORD\n      valueFrom:\n        secretKeyRef:\n          name: db-creds\n          key: password";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn allows_non_sensitive_literal() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    env:\n    - name: LOG_LEVEL\n      value: info";
        assert!(run(yaml).is_empty());
    }
}
