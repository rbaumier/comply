//! k8s-require-network-policy tree-sitter backend (YAML AST).
//!
//! Emits once per Deployment in the file when no NetworkPolicy document is
//! present anywhere in the same file. Walks the tree once to decide and
//! reports from a root-level visit so we don't double-count.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { on ["stream"] => |node, source, ctx, diagnostics|
    let tree_docs = document_manifests(node, source);
    if tree_docs.is_empty() {
        return;
    }
    let has_network_policy = tree_docs
        .iter()
        .any(|(_, kind)| kind == "NetworkPolicy");
    if has_network_policy {
        return;
    }
    for (manifest, kind) in &tree_docs {
        if kind == "Deployment" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                manifest,
                super::META.id,
                "Deployment should be paired with a NetworkPolicy to restrict traffic.".into(),
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
        let Some(mapping) = y::top_mapping_of_document(document) else { continue; };
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
    fn flags_deployment_without_netpol() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec: {}";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_deployment_with_netpol() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nspec: {}\n---\napiVersion: networking.k8s.io/v1\nkind: NetworkPolicy\nspec: {}";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_deployment() {
        let yaml = "apiVersion: v1\nkind: Service\nspec: {}";
        assert!(run(yaml).is_empty());
    }
}
