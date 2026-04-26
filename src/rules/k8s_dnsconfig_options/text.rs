//! k8s-dnsconfig-options tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if !matches!(kind.as_str(), "Pod" | "Deployment" | "StatefulSet" | "DaemonSet") { return; }
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };

    let has_options = y::descend_sequence(pod_spec, source, &["dnsConfig", "options"]).is_some();
    if has_options { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Pod spec is missing `dnsConfig.options`; set `ndots:2` to reduce DNS lookup latency.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_yaml(s, &Check)
    }

    #[test]
    fn flags_missing_dnsconfig() {
        let yaml =
            "apiVersion: v1\nkind: Pod\nspec:\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_dnsconfig_without_options() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  dnsConfig:\n    nameservers:\n    - 1.1.1.1\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_dnsconfig_with_options() {
        let yaml = "apiVersion: v1\nkind: Pod\nspec:\n  dnsConfig:\n    options:\n    - name: ndots\n      value: \"2\"\n  containers:\n  - name: app\n    image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
