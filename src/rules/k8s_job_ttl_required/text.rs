//! k8s-job-ttl-required tree-sitter backend (YAML AST).
//!
//! Flags `kind: Job` manifests missing `spec.ttlSecondsAfterFinished`.
//! Without it, completed Jobs accumulate indefinitely.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Job" {
        return;
    }
    let Some(spec) = y::descend_mapping(node, source, &["spec"]) else {
        let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &kind_pair,
            super::META.id,
            "Job must set spec.ttlSecondsAfterFinished for cleanup.".into(),
            Severity::Warning,
        ));
        return;
    };
    if y::find_pair(spec, source, "ttlSecondsAfterFinished").is_none() {
        let kind_pair = y::find_pair(node, source, "kind").unwrap_or(node);
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &kind_pair,
            super::META.id,
            "Job must set spec.ttlSecondsAfterFinished for cleanup.".into(),
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
    fn flags_missing_ttl() {
        let yaml = "apiVersion: batch/v1\nkind: Job\nspec:\n  template:\n    spec:\n      containers:\n      - name: job\n        image: busybox:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_explicit_ttl() {
        let yaml = "apiVersion: batch/v1\nkind: Job\nspec:\n  ttlSecondsAfterFinished: 3600\n  template:\n    spec:\n      containers:\n      - name: job\n        image: busybox:1.0";
        assert!(run(yaml).is_empty());
    }
}
