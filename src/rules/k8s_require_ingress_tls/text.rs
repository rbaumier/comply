//! k8s-require-ingress-tls tree-sitter backend (YAML AST).
//!
//! An `Ingress` must define a non-empty `spec.tls` list (block or flow).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Ingress" {
        return;
    }
    if has_populated_tls(node, source) {
        return;
    }
    let anchor = y::find_pair(node, source, "kind").unwrap_or(node);
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &anchor,
        super::META.id,
        "Ingress must define spec.tls to terminate TLS.".into(),
        Severity::Warning,
    ));
}

fn has_populated_tls(manifest: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(spec) = y::descend_mapping(manifest, source, &["spec"]) else { return false; };
    let Some(pair) = y::find_pair(spec, source, "tls") else { return false; };
    let Some(value) = y::pair_value_node(pair) else { return false; };
    sequence_has_item(value)
}

fn sequence_has_item(value: tree_sitter::Node) -> bool {
    match value.kind() {
        "block_node" | "flow_node" => {
            let mut cursor = value.walk();
            value.named_children(&mut cursor).any(sequence_has_item)
        }
        "block_sequence" => {
            let mut cursor = value.walk();
            value
                .named_children(&mut cursor)
                .any(|c| c.kind() == "block_sequence_item")
        }
        "flow_sequence" => {
            // Any non-zero children — note flow_sequence includes `[` and `]` tokens
            // as anonymous children, so we need named_children only.
            let mut cursor = value.walk();
            value.named_children(&mut cursor).count() > 0
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_missing_tls() {
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: Ingress\nspec:\n  rules:\n  - host: example.com";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_empty_tls() {
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: Ingress\nspec:\n  tls: []";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_populated_tls() {
        let yaml = "apiVersion: networking.k8s.io/v1\nkind: Ingress\nspec:\n  tls:\n  - hosts:\n    - example.com\n    secretName: ex-tls";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_ingress() {
        let yaml = "apiVersion: v1\nkind: Service\nspec: {}";
        assert!(run(yaml).is_empty());
    }
}
