//! k8s-no-exposed-services tree-sitter backend (YAML AST).
//!
//! Flags `Service` manifests whose `spec.type` is `NodePort` or
//! `LoadBalancer` — both expose pods outside the cluster network.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Service" {
        return;
    }
    let Some(spec) = y::descend_mapping(node, source, &["spec"]) else { return; };
    let Some(type_pair) = y::find_pair(spec, source, "type") else { return; };
    let Some(type_value) = y::pair_scalar_value(type_pair, source) else { return; };
    if type_value == "NodePort" || type_value == "LoadBalancer" {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &type_pair,
            super::META.id,
            format!("Service type `{type_value}` exposes pods outside the cluster; use ClusterIP + Ingress/Gateway instead."),
            Severity::Warning,
        ));
    }
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
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "manifest.yaml")
    }

    #[test]
    fn flags_node_port() {
        let yaml = "apiVersion: v1\nkind: Service\nspec:\n  type: NodePort";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_load_balancer() {
        let yaml = "apiVersion: v1\nkind: Service\nspec:\n  type: LoadBalancer";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_cluster_ip() {
        let yaml = "apiVersion: v1\nkind: Service\nspec:\n  type: ClusterIP";
        assert!(run(yaml).is_empty());
    }
}
