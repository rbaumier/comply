//! k8s-rbac-no-wildcard-verbs tree-sitter backend (YAML AST).
//!
//! For every entry in `rules[]` of a `Role` / `ClusterRole`, flag the
//! `verbs:` list if it contains `"*"`. Reuses the wildcard-detection
//! helper that lives in the sibling `k8s_rbac_no_wildcard_resources` rule.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::k8s_rbac_no_wildcard_resources::list_contains_star;
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Role" && kind != "ClusterRole" {
        return;
    }
    let Some(rules_seq) = y::descend_sequence(node, source, &["rules"]) else { return; };
    for rule_map in y::sequence_item_mappings(rules_seq) {
        let Some(pair) = y::find_pair(rule_map, source, "verbs") else { continue; };
        if list_contains_star(pair, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &pair,
                super::META.id,
                "RBAC rule grants verbs: [\"*\"]; enumerate the verbs needed.".into(),
                Severity::Warning,
            ));
        }
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
    fn flags_flow_wildcard() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: Role\nrules:\n- verbs: [\"*\"]";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_block_wildcard() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nrules:\n- verbs:\n  - \"*\"";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_enumerated_verbs() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: Role\nrules:\n- verbs: [\"get\", \"list\"]";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_non_rbac() {
        let yaml = "apiVersion: v1\nkind: Service\nrules:\n- verbs: [\"*\"]";
        assert!(run(yaml).is_empty());
    }
}
