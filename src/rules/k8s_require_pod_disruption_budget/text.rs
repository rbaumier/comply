//! k8s-require-pod-disruption-budget tree-sitter backend (YAML AST).
//!
//! Emits once per Deployment/StatefulSet in the file when no
//! PodDisruptionBudget document is present in the same file. Same-file
//! presence silences — we don't attempt cross-file analysis.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { on ["stream"] prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    let tree_docs = document_manifests(node, source);
    if tree_docs.is_empty() {
        return;
    }
    let has_pdb = tree_docs
        .iter()
        .any(|(_, kind)| kind == "PodDisruptionBudget");
    if has_pdb {
        return;
    }
    for (manifest, kind) in &tree_docs {
        if kind == "Deployment" || kind == "StatefulSet" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                manifest,
                super::META.id,
                "Deployment/StatefulSet should have a PodDisruptionBudget to survive voluntary disruptions.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn document_manifests<'t>(
    stream: tree_sitter::Node<'t>,
    source: &[u8],
) -> Vec<(tree_sitter::Node<'t>, String)> {
    let mut out = Vec::new();
    let mut cursor = stream.walk();
    for document in stream.named_children(&mut cursor) {
        if document.kind() != "document" {
            continue;
        }
        let Some(mapping) = y::top_mapping_of_document(document) else {
            continue;
        };
        if !y::is_k8s_manifest_mapping(mapping, source) {
            continue;
        }
        if let Some(kind) = y::manifest_kind(mapping, source) {
            out.push((mapping, kind));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_deployment_without_pdb() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec: {}";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_statefulset_without_pdb() {
        let yaml = "apiVersion: apps/v1\nkind: StatefulSet\nspec: {}";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_deployment_with_pdb_in_file() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec: {}\n---\napiVersion: policy/v1\nkind: PodDisruptionBudget\nspec:\n  minAvailable: 1";
        assert!(run(yaml).is_empty());
    }
}
