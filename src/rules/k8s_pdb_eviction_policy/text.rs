//! k8s-pdb-eviction-policy tree-sitter backend (YAML AST).
//!
//! Flags `PodDisruptionBudget` manifests that omit
//! `spec.unhealthyPodEvictionPolicy`. Without it, unhealthy pods can
//! block voluntary disruptions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "PodDisruptionBudget" {
        return;
    }
    let Some(spec) = y::descend_mapping(node, source, &["spec"]) else {
        let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &kind_pair,
            super::META.id,
            "PodDisruptionBudget must set spec.unhealthyPodEvictionPolicy.".into(),
            Severity::Warning,
        ));
        return;
    };
    if y::find_pair(spec, source, "unhealthyPodEvictionPolicy").is_none() {
        let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &kind_pair,
            super::META.id,
            "PodDisruptionBudget must set spec.unhealthyPodEvictionPolicy.".into(),
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
    fn flags_missing_eviction_policy() {
        let yaml = "apiVersion: policy/v1\nkind: PodDisruptionBudget\nspec:\n  minAvailable: 1";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_explicit_eviction_policy() {
        let yaml = "apiVersion: policy/v1\nkind: PodDisruptionBudget\nspec:\n  minAvailable: 1\n  unhealthyPodEvictionPolicy: AlwaysAllow";
        assert!(run(yaml).is_empty());
    }
}
