//! k8s-no-host-network tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    let Some(pair) = y::find_pair(pod_spec, source, "hostNetwork") else { return; };
    if y::pair_scalar_value(pair, source).as_deref() == Some("true") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &pair,
            super::META.id,
            "Pod spec sets `hostNetwork: true`; remove to isolate the network namespace.".into(),
            Severity::Warning,
        ));
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
    fn flags_host_network_true() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  hostNetwork: true\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_default() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
