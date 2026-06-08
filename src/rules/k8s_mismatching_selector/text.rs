//! k8s-mismatching-selector tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;
use tree_sitter::Node;

fn collect_pairs(mapping: Node, source: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = mapping.walk();
    for child in mapping.named_children(&mut cursor) {
        if child.kind() != "block_mapping_pair" {
            continue;
        }
        let Some(key) = y::pair_key_text(child, source) else {
            continue;
        };
        let Some(val) = y::pair_scalar_value(child, source) else {
            continue;
        };
        out.push((key, val));
    }
    out
}

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if !matches!(kind.as_str(), "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet") {
        return;
    }
    let Some(match_labels) =
        y::descend_mapping(node, source, &["spec", "selector", "matchLabels"])
    else { return; };
    let Some(template_labels) =
        y::descend_mapping(node, source, &["spec", "template", "metadata", "labels"])
    else { return; };

    let selector_pairs = collect_pairs(match_labels, source);
    let template_pairs = collect_pairs(template_labels, source);

    let mut mismatch = false;
    for (k, v) in &selector_pairs {
        let found = template_pairs
            .iter()
            .any(|(tk, tv)| tk == k && tv == v);
        if !found {
            mismatch = true;
            break;
        }
    }

    if mismatch {
        // Flag the selector mapping node.
        let Some(selector) = y::descend_mapping(node, source, &["spec", "selector"])
        else { return; };
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &selector,
            super::META.id,
            "Selector matchLabels do not match spec.template.metadata.labels; pods will not be selected.".into(),
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
    
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "manifest.yaml")
    }

    #[test]
    fn flags_mismatching_labels() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  selector:\n    matchLabels:\n      app: foo\n  template:\n    metadata:\n      labels:\n        app: bar\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_matching_labels() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec:\n  selector:\n    matchLabels:\n      app: foo\n  template:\n    metadata:\n      labels:\n        app: foo\n        tier: web\n    spec:\n      containers:\n      - name: app\n        image: nginx:1.0";
        assert!(run(yaml).is_empty());
    }
}
