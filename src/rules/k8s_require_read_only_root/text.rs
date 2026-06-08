//! k8s-require-read-only-root tree-sitter backend (YAML AST).
//!
//! Flags any container whose `securityContext.readOnlyRootFilesystem` is
//! absent or not set to `true`. Init containers are checked in addition to
//! main containers because they share the pod's filesystem and a writable
//! init step can still modify it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    let Some(pod_spec) = y::pod_spec_mapping(node, source, &kind) else { return; };
    for container in y::containers_of_pod_spec(pod_spec, source, true) {
        if !has_read_only_root_true(container, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &container,
                super::META.id,
                "Container must set securityContext.readOnlyRootFilesystem: true.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn has_read_only_root_true(container: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(sec) = y::descend_mapping(container, source, &["securityContext"]) else {
        return false;
    };
    let Some(pair) = y::find_pair(sec, source, "readOnlyRootFilesystem") else {
        return false;
    };
    y::pair_scalar_value(pair, source).as_deref() == Some("true")
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
    fn flags_missing_field() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_true_value() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0\n        securityContext:\n          readOnlyRootFilesystem: true";
        assert!(run(yaml).is_empty());
    }
}
