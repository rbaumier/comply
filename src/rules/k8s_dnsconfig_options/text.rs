//! k8s-dnsconfig-options tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "manifest.yaml")
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
