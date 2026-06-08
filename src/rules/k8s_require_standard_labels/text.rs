//! k8s-require-standard-labels tree-sitter backend (YAML AST).
//!
//! Every k8s manifest must declare both `app.kubernetes.io/name` and
//! `app.kubernetes.io/instance` under top-level `metadata.labels`.
//! Nested pod-template metadata (spec.template.metadata) is ignored — we
//! only inspect the manifest's own root metadata.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

const REQUIRED_LABELS: &[&str] = &["app.kubernetes.io/name", "app.kubernetes.io/instance"];

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let labels = y::descend_mapping(node, source, &["metadata", "labels"]);
    let (has_name, has_instance, anchor) = match labels {
        Some(labels_mapping) => {
            let has_name = y::has_key(labels_mapping, source, REQUIRED_LABELS[0]);
            let has_instance = y::has_key(labels_mapping, source, REQUIRED_LABELS[1]);
            (has_name, has_instance, labels_mapping)
        }
        None => (false, false, node),
    };
    if !has_name || !has_instance {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &anchor,
            super::META.id,
            "Resource must include app.kubernetes.io/name and app.kubernetes.io/instance labels.".into(),
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
    fn flags_missing_labels() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: app";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_partial_labels() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: app\n  labels:\n    app.kubernetes.io/name: app";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_full_labels() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: app\n  labels:\n    app.kubernetes.io/name: app\n    app.kubernetes.io/instance: app-prod";
        assert!(run(yaml).is_empty());
    }
}
