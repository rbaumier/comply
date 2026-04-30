//! Shared tree-sitter-yaml helpers for Kubernetes manifest rules.
//!
//! All k8s rules walk the same YAML AST shape:
//!
//! - a `stream` root node with one or more `document` children;
//! - each `document` holds a `block_node` → `block_mapping` with
//!   `apiVersion`, `kind`, `metadata`, `spec` pairs;
//! - nested mappings are `block_node` → `block_mapping` sequences;
//! - lists (`containers`, `volumes`, …) are `block_node` → `block_sequence`
//!   → `block_sequence_item` → `block_node` → `block_mapping`.
//!
//! The helpers below wrap the repetitive "given a mapping, find this key,
//! follow it into its nested mapping" traversal so each rule's body stays
//! focused on the k8s semantics (is it a Deployment? does the container
//! have a livenessProbe?) rather than AST mechanics.

use tree_sitter::Node;

/// True if `node` is a `block_mapping` whose key set identifies it as a
/// Kubernetes manifest (has both `apiVersion` and `kind` at the top level).
#[must_use]
pub fn is_k8s_manifest_mapping(node: Node, source: &[u8]) -> bool {
    node.kind() == "block_mapping"
        && has_key(node, source, "apiVersion")
        && has_key(node, source, "kind")
}

/// Return the `block_mapping` inside a `document` node (skipping the
/// intermediate `block_node`), if any.
#[must_use]
pub fn top_mapping_of_document(document: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = document.walk();
    let block_node = document
        .named_children(&mut cursor)
        .find(|c| c.kind() == "block_node")?;
    let mut cursor = block_node.walk();
    block_node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "block_mapping")
}

/// Does `mapping` (a `block_mapping`) contain a pair whose key equals `key`?
#[must_use]
pub fn has_key(mapping: Node, source: &[u8], key: &str) -> bool {
    find_pair(mapping, source, key).is_some()
}

/// Find the `block_mapping_pair` under `mapping` whose key equals `key`.
#[must_use]
pub fn find_pair<'t>(mapping: Node<'t>, source: &[u8], key: &str) -> Option<Node<'t>> {
    let mut cursor = mapping.walk();
    mapping.named_children(&mut cursor).find(|child| {
        child.kind() == "block_mapping_pair"
            && pair_key_text(*child, source).as_deref() == Some(key)
    })
}

/// Key text of a `block_mapping_pair`, stripped of wrapping quotes.
#[must_use]
pub fn pair_key_text(pair: Node, source: &[u8]) -> Option<String> {
    let key_node = pair.named_child(0)?;
    let raw = key_node.utf8_text(source).ok()?;
    Some(raw.trim().trim_matches('"').trim_matches('\'').to_string())
}

/// Value text of a `block_mapping_pair` when the value is a scalar.
/// Returns `None` for nested mapping / sequence values.
#[must_use]
pub fn pair_scalar_value(pair: Node, source: &[u8]) -> Option<String> {
    let value_node = pair.named_child(1)?;
    if value_node.kind() != "flow_node" {
        return None;
    }
    let raw = value_node.utf8_text(source).ok()?;
    Some(raw.trim().trim_matches('"').trim_matches('\'').to_string())
}

/// Value node (2nd named child) of a `block_mapping_pair`, whatever its kind.
#[must_use]
pub fn pair_value_node<'t>(pair: Node<'t>) -> Option<Node<'t>> {
    pair.named_child(1)
}

/// Unwrap `flow_node` / `block_node` indirections to reach a `block_mapping`.
#[must_use]
pub fn as_mapping<'t>(node: Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "block_mapping" {
        return Some(node);
    }
    if node.kind() == "block_node" {
        let mut cursor = node.walk();
        return node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "block_mapping");
    }
    None
}

/// Unwrap indirections to reach a `block_sequence`.
#[must_use]
pub fn as_sequence<'t>(node: Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "block_sequence" {
        return Some(node);
    }
    if node.kind() == "block_node" {
        let mut cursor = node.walk();
        return node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "block_sequence");
    }
    None
}

/// Iterate the inner `block_mapping` of each `block_sequence_item`, skipping
/// items whose value isn't a mapping.
#[must_use]
pub fn sequence_item_mappings<'t>(sequence: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut cursor = sequence.walk();
    for item in sequence.named_children(&mut cursor) {
        if item.kind() != "block_sequence_item" {
            continue;
        }
        let mut inner = item.walk();
        if let Some(block_node) = item
            .named_children(&mut inner)
            .find(|c| c.kind() == "block_node")
            && let Some(mapping) = as_mapping(block_node)
        {
            out.push(mapping);
        }
    }
    out
}

/// Starting from a top-level manifest `block_mapping`, return the `kind`
/// scalar value (e.g. `Deployment`, `Service`, `NetworkPolicy`).
#[must_use]
pub fn manifest_kind(manifest: Node, source: &[u8]) -> Option<String> {
    let pair = find_pair(manifest, source, "kind")?;
    pair_scalar_value(pair, source)
}

/// Walk a path of keys through nested block mappings. Each step unwraps
/// `block_node` / `block_mapping` indirections as needed. Returns the
/// mapping node at the end of the path, or `None` if any step is missing
/// or the terminal value is not a mapping.
#[must_use]
pub fn descend_mapping<'t>(mapping: Node<'t>, source: &[u8], path: &[&str]) -> Option<Node<'t>> {
    let mut current = mapping;
    for key in path {
        let pair = find_pair(current, source, key)?;
        let value = pair_value_node(pair)?;
        current = as_mapping(value)?;
    }
    Some(current)
}

/// Walk a path and return the terminal `block_sequence`, if any.
#[must_use]
pub fn descend_sequence<'t>(mapping: Node<'t>, source: &[u8], path: &[&str]) -> Option<Node<'t>> {
    if path.is_empty() {
        return None;
    }
    let (last, prefix) = path.split_last()?;
    let parent = if prefix.is_empty() {
        mapping
    } else {
        descend_mapping(mapping, source, prefix)?
    };
    let pair = find_pair(parent, source, last)?;
    let value = pair_value_node(pair)?;
    as_sequence(value)
}

/// Locate the pod-template spec mapping for a workload manifest. Covers
/// `Deployment`/`StatefulSet`/`DaemonSet`/`ReplicaSet` (`spec.template.spec`),
/// `Job` (`spec.template.spec`), `CronJob` (`spec.jobTemplate.spec.template.spec`),
/// and a bare `Pod` (`spec`). Returns `None` if the manifest kind doesn't
/// carry a pod spec.
#[must_use]
pub fn pod_spec_mapping<'t>(manifest: Node<'t>, source: &[u8], kind: &str) -> Option<Node<'t>> {
    match kind {
        "Pod" => descend_mapping(manifest, source, &["spec"]),
        "CronJob" => descend_mapping(
            manifest,
            source,
            &["spec", "jobTemplate", "spec", "template", "spec"],
        ),
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" => {
            descend_mapping(manifest, source, &["spec", "template", "spec"])
        }
        _ => None,
    }
}

/// Return every container mapping under a pod spec. Optionally include
/// init containers.
#[must_use]
pub fn containers_of_pod_spec<'t>(
    pod_spec: Node<'t>,
    source: &[u8],
    include_init: bool,
) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    if let Some(seq) = descend_sequence(pod_spec, source, &["containers"]) {
        out.extend(sequence_item_mappings(seq));
    }
    if include_init && let Some(seq) = descend_sequence(pod_spec, source, &["initContainers"]) {
        out.extend(sequence_item_mappings(seq));
    }
    out
}
