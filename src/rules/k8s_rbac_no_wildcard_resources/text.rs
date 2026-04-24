//! k8s-rbac-no-wildcard-resources tree-sitter backend (YAML AST).
//!
//! For every entry in `rules[]` of a `Role` / `ClusterRole`, flag the
//! `resources:` list if it contains `"*"`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) {
        return;
    }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Role" && kind != "ClusterRole" {
        return;
    }
    let Some(rules_seq) = y::descend_sequence(node, source, &["rules"]) else { return; };
    for rule_map in y::sequence_item_mappings(rules_seq) {
        let Some(pair) = y::find_pair(rule_map, source, "resources") else { continue; };
        if super::list_contains_star(pair, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &pair,
                super::META.id,
                "RBAC rule grants resources: [\"*\"]; enumerate the resources needed.".into(),
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
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nrules:\n- resources: [\"*\"]\n  verbs: [\"get\"]";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_block_wildcard() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: Role\nrules:\n- resources:\n  - \"*\"\n  verbs: [\"get\"]";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_enumerated_resources() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: Role\nrules:\n- resources: [\"pods\", \"services\"]\n  verbs: [\"get\"]";
        assert!(run(yaml).is_empty());
    }
}
