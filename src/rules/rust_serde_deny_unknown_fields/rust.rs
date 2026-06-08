//! rust-serde-deny-unknown-fields backend.
//!
//! For every `struct_item` with a `#[derive(..., Deserialize, ...)]`
//! attribute, scan the preceding attribute siblings for
//! `#[serde(deny_unknown_fields)]`. If absent, flag the struct.
//!
//! **Exception:** a struct with any `#[serde(flatten)]` field is
//! deliberately NOT flagged. `deny_unknown_fields` and `flatten` are
//! mutually exclusive in serde — the flatten's target FxHashMap/struct
//! is exactly the mechanism for accepting unknown keys, so rejecting
//! them before the flatten can catch them defeats the field's purpose.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["struct_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        let source_bytes = ctx.source.as_bytes();
        let attrs = collect_preceding_attrs(node, source_bytes);
        if !attrs.iter().any(|a| derives_deserialize(a)) {
            return;
        }
        if attrs.iter().any(|a| has_deny_unknown_fields(a)) {
            return;
        }
        // Structs with a `#[serde(flatten)]` field cannot have
        // `deny_unknown_fields` — the two are mutually exclusive.
        if has_flatten_field(node, source_bytes) {
            return;
        }
        // ORM structs (Diesel Queryable / Selectable) deserialize from
        // internal query results, not user input — forward-compat is
        // more important than strict field validation.
        if has_orm_derive(&attrs) {
            return;
        }
        // Structs marked with a forward-compat doc comment are mirrors
        // of external contracts we don't own. Accepted marker phrases:
        //   "external wire format mirror" (legacy)
        //   "external api response"
        //   "versioned protocol"
        if has_forward_compat_marker(node, source_bytes) {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("Struct");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-serde-deny-unknown-fields".into(),
            message: format!(
                "`{name}` derives `Deserialize` but is missing \
                 `#[serde(deny_unknown_fields)]` — typos in input \
                 fields will be silently dropped. Add the attribute \
                 to catch unknown keys at parse time."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn collect_preceding_attrs(item: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    // Walk every preceding sibling; keep going through attribute_item
    // and interleaved comment nodes. tree-sitter-rust inserts a
    // `line_comment`/`block_comment` sibling whenever an attribute has
    // a trailing `//` note (e.g. `#[allow(dead_code)] // explanation`),
    // so stopping at the first non-attribute would prematurely end the
    // block and miss derives sitting above it.
    let mut out = Vec::new();
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source) {
                    out.push(text.to_string());
                }
            }
            "line_comment" | "block_comment" => {
                // Interleaved comment — keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    out
}

fn derives_deserialize(attr_text: &str) -> bool {
    // Match `#[derive(..., Deserialize, ...)]` — we don't enforce
    // word boundaries because `MyDeserialize` would be a very strange
    // name to invent.
    attr_text.contains("derive(") && attr_text.contains("Deserialize")
}

fn has_deny_unknown_fields(attr_text: &str) -> bool {
    attr_text.contains("deny_unknown_fields")
}

fn has_orm_derive(attrs: &[String]) -> bool {
    attrs
        .iter()
        .any(|a| a.contains("derive(") && (a.contains("Queryable") || a.contains("Selectable")))
}

/// True if the struct's preceding doc comments contain any of the
/// forward-compat marker phrases:
/// - `"external wire format mirror"` (legacy)
/// - `"external api response"` — GitHub/Svix-style API mirrors
/// - `"versioned protocol"` — DAP, dump readers, forward-compat formats
fn has_forward_compat_marker(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {
                if let Ok(text) = s.utf8_text(source) {
                    let lowered = text.to_ascii_lowercase();
                    if lowered.contains("external wire format mirror")
                        || lowered.contains("external api response")
                        || lowered.contains("versioned protocol")
                    {
                        return true;
                    }
                }
            }
            "attribute_item" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if any field inside the struct body carries a
/// `#[serde(flatten)]` attribute. We walk the `field_declaration_list`
/// child and, for each `field_declaration`, look for preceding
/// `attribute_item` siblings containing `flatten`.
fn has_flatten_field(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = struct_node.child_by_field_name("body") else {
        return false;
    };
    if body.kind() != "field_declaration_list" {
        return false;
    }
    let mut cursor = body.walk();
    for field in body.children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        let mut sibling = field.prev_named_sibling();
        while let Some(s) = sibling {
            match s.kind() {
                "attribute_item" => {
                    if let Ok(text) = s.utf8_text(source)
                        && text.contains("flatten")
                    {
                        return true;
                    }
                }
                "line_comment" | "block_comment" => {}
                _ => break,
            }
            sibling = s.prev_named_sibling();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_deserialize_without_deny_unknown_fields() {
        let source = "#[derive(Deserialize)]\nstruct Config { rate: u32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_deserialize_with_deny_unknown_fields() {
        let source =
            "#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct Config { rate: u32 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_without_deserialize() {
        let source = "#[derive(Debug)]\nstruct Config { rate: u32 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_mixed_derive_with_deserialize() {
        let source = "#[derive(Debug, Clone, Deserialize, Serialize)]\nstruct Config { rate: u32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_struct_with_flatten_field() {
        // `deny_unknown_fields` and `#[serde(flatten)]` are mutually
        // exclusive — the flatten is how you accept unknown keys.
        let source = "#[derive(Deserialize)]\n\
                      struct Config {\n\
                          name: String,\n\
                          #[serde(flatten)]\n\
                          extra: rustc_hash::FxHashMap<String, toml::Value>,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "false positive: struct with flatten field can't have deny_unknown_fields"
        );
    }

    #[test]
    fn allows_queryable_orm_struct() {
        let source = "#[derive(Debug, Deserialize, Queryable)]\nstruct User { id: i32, name: String }";
        assert!(run_on(source).is_empty(), "FP: ORM struct flagged despite Queryable");
    }

    #[test]
    fn allows_selectable_orm_struct() {
        let source = "#[derive(Deserialize, Selectable)]\nstruct User { id: i32 }";
        assert!(run_on(source).is_empty(), "FP: ORM struct flagged despite Selectable");
    }

    #[test]
    fn allows_external_api_response_with_marker() {
        let source = "// external api response — version-compatible\n#[derive(Deserialize)]\nstruct GithubUser { login: String }";
        assert!(run_on(source).is_empty(), "FP: external API response flagged");
    }

    #[test]
    fn allows_versioned_protocol_with_marker() {
        let source = "// versioned protocol — accepts future fields\n#[derive(Deserialize)]\nstruct DapMessage { seq: i32 }";
        assert!(run_on(source).is_empty(), "FP: versioned protocol flagged");
    }

    #[test]
    fn flags_despite_incidental_api_mention() {
        // "external api" alone does NOT trigger the exemption — must be "external api response"
        let source = "// fetches data from external api of payment provider\n#[derive(Deserialize)]\nstruct PaymentData { amount: u64 }";
        assert_eq!(run_on(source).len(), 1, "should still flag: comment mentions external api but not 'external api response'");
    }
}
