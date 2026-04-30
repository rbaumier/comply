//! k8s-no-plaintext-secret-in-git tree-sitter backend (YAML AST).
//!
//! Flags a `Secret` manifest when `data:` or `stringData:` has any populated
//! child key (the secret value is stored in plaintext in the source tree).
//! `SealedSecret` and other kinds are not covered here.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Secret" {
        return;
    }
    for key in ["data", "stringData"] {
        let Some(pair) = y::find_pair(node, source, key) else { continue; };
        if has_any_child_pair(pair, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &pair,
                super::META.id,
                "Secret has populated data/stringData in source; use sealed/external secrets instead.".into(),
                Severity::Warning,
            ));
        }
    }
}

/// True when the pair's value is a block/flow mapping containing at least
/// one `block_mapping_pair` child.
fn has_any_child_pair(pair: tree_sitter::Node, _source: &[u8]) -> bool {
    let Some(value) = y::pair_value_node(pair) else {
        return false;
    };
    let Some(mapping) = y::as_mapping(value) else {
        return false;
    };
    let mut cursor = mapping.walk();
    mapping
        .named_children(&mut cursor)
        .any(|c| c.kind() == "block_mapping_pair")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_yaml;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml(source, &Check)
    }

    #[test]
    fn flags_data_populated() {
        let yaml =
            "apiVersion: v1\nkind: Secret\nmetadata:\n  name: s\ndata:\n  password: aHVudGVyMg==";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_stringdata_populated() {
        let yaml = "apiVersion: v1\nkind: Secret\nstringData:\n  token: abc";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_empty_secret() {
        let yaml = "apiVersion: v1\nkind: Secret\nmetadata:\n  name: s\ntype: Opaque";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_sealed_secret() {
        let yaml = "apiVersion: bitnami.com/v1alpha1\nkind: SealedSecret\nspec:\n  encryptedData:\n    token: XYZ";
        assert!(run(yaml).is_empty());
    }
}
