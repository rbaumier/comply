//! k8s-rbac-no-create-pods tree-sitter backend (YAML AST).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers as y;

crate::ast_check! { prefilter = ["apiVersion"] => |node, source, ctx, diagnostics|
    if !y::is_k8s_manifest_mapping(node, source) { return; }
    let Some(kind) = y::manifest_kind(node, source) else { return; };
    if kind != "Role" && kind != "ClusterRole" { return; }
    let Some(rules_seq) = y::descend_sequence(node, source, &["rules"]) else { return; };
    for rule_map in y::sequence_item_mappings(rules_seq) {
        let Some(resources_pair) = y::find_pair(rule_map, source, "resources") else { continue; };
        let Some(verbs_pair) = y::find_pair(rule_map, source, "verbs") else { continue; };
        if seq_contains_value(resources_pair, source, "pods")
            && seq_contains_value(verbs_pair, source, "create")
        {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &rule_map,
                super::META.id,
                "RBAC rule grants `create` on `pods`; this enables privilege escalation.".into(),
                Severity::Warning,
            ));
        }
    }
}

fn seq_contains_value(pair: tree_sitter::Node, source: &[u8], target: &str) -> bool {
    let Some(value) = y::pair_value_node(pair) else {
        return false;
    };
    check_seq_value(value, source, target)
}

fn check_seq_value(node: tree_sitter::Node, source: &[u8], target: &str) -> bool {
    match node.kind() {
        "flow_node" | "block_node" => {
            let mut c = node.walk();
            node.named_children(&mut c)
                .any(|ch| check_seq_value(ch, source, target))
        }
        "flow_sequence" => {
            let mut c = node.walk();
            node.named_children(&mut c).any(|ch| {
                ch.utf8_text(source)
                    .ok()
                    .is_some_and(|t| t.trim().trim_matches('"').trim_matches('\'') == target)
            })
        }
        "block_sequence" => {
            let mut c = node.walk();
            for item in node.named_children(&mut c) {
                if item.kind() == "block_sequence_item" {
                    let mut ic = item.walk();
                    for ichild in item.named_children(&mut ic) {
                        if ichild.utf8_text(source).ok().is_some_and(|t| {
                            t.trim().trim_matches('"').trim_matches('\'') == target
                        }) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        _ => false,
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
    fn flags_create_pods() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nrules:\n- resources: [\"pods\"]\n  verbs: [\"create\"]";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_get_list_pods() {
        let yaml = "apiVersion: rbac.authorization.k8s.io/v1\nkind: ClusterRole\nrules:\n- resources: [\"pods\"]\n  verbs: [\"get\", \"list\"]";
        assert!(run(yaml).is_empty());
    }
}
