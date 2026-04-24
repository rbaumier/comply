//! k8s-rbac-no-wildcard-resources — Role/ClusterRole must not use resources: ["*"].

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::yaml_k8s_helpers as y;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-rbac-no-wildcard-resources",
    description: "RBAC rules must not grant resources: [\"*\"]; enumerate the resources needed.",
    remediation: "Replace `resources: [\"*\"]` with the specific resources required (pods, services, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["kubernetes"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}

/// True when the value of `pair` is a sequence containing the scalar `"*"`.
/// Handles both flow (`[X, "*"]`) and block (`- "*"`) list styles.
pub(super) fn list_contains_star(pair: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = y::pair_value_node(pair) else { return false; };
    contains_star(value, source)
}

fn contains_star(value: tree_sitter::Node, source: &[u8]) -> bool {
    match value.kind() {
        "block_node" | "flow_node" => {
            let mut cursor = value.walk();
            value
                .named_children(&mut cursor)
                .any(|c| contains_star(c, source))
        }
        "block_sequence" => {
            let mut cursor = value.walk();
            for item in value.named_children(&mut cursor) {
                if item.kind() == "block_sequence_item" {
                    let mut icur = item.walk();
                    for ichild in item.named_children(&mut icur) {
                        if scalar_is_star(ichild, source) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        "flow_sequence" => {
            let mut cursor = value.walk();
            value
                .named_children(&mut cursor)
                .any(|c| scalar_is_star(c, source))
        }
        _ => scalar_is_star(value, source),
    }
}

fn scalar_is_star(node: tree_sitter::Node, source: &[u8]) -> bool {
    let raw = match node.utf8_text(source) {
        Ok(s) => s,
        Err(_) => return false,
    };
    raw.trim().trim_matches('"').trim_matches('\'') == "*"
}
