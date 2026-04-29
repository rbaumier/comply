//! k8s-no-duplicate-env-vars tree-sitter backend (YAML AST).
//!
//! For each container, collects env entry names and flags subsequent
//! occurrences of the same name.

use std::collections::HashSet;

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
        let mut seen: HashSet<String> = HashSet::new();
        for entry in y::sequence_item_mappings(env) {
            let Some(name_pair) = y::find_pair(entry, source, "name") else { continue; };
            let Some(name) = y::pair_scalar_value(name_pair, source) else { continue; };
            if !seen.insert(name.clone()) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &name_pair,
                    super::META.id,
                    format!("Duplicate env var `{name}` — later entries silently override earlier ones."),
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
    fn flags_duplicate_env_name() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    env:\n    - name: FOO\n      value: bar\n    - name: FOO\n      value: baz";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_unique_env_names() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0\n    env:\n    - name: FOO\n      value: bar\n    - name: BAZ\n      value: qux";
        assert!(run(yaml).is_empty());
    }
}
