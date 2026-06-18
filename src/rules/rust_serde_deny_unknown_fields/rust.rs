//! rust-serde-deny-unknown-fields backend.
//!
//! For every named-field `struct_item` with a
//! `#[derive(..., Deserialize, ...)]` attribute, scan the preceding
//! attribute siblings for `#[serde(deny_unknown_fields)]`. If absent,
//! flag the struct.
//!
//! Only named-field structs (`field_declaration_list` body) are checked.
//! Tuple / newtype structs (`ordered_field_declaration_list`) and unit
//! structs (no body) deserialize via the inner type's deserializer with
//! no field-name map, so `deny_unknown_fields` is inert and they are
//! never flagged.
//!
//! **Exception:** a struct with any `#[serde(flatten)]` field is
//! deliberately NOT flagged. `deny_unknown_fields` and `flatten` are
//! mutually exclusive in serde — the flatten's target HashMap/struct
//! is exactly the mechanism for accepting unknown keys, so rejecting
//! them before the flatten can catch them defeats the field's purpose.
//!
//! **Exception:** a `#[serde(transparent)]` struct is NOT flagged. It
//! delegates all (de)serialization to its single inner field and has no
//! field-name map of its own, so `deny_unknown_fields` is a no-op there.
//!
//! **Exception:** structs defined inside a test context (a `#[test]`
//! function, a path-qualified test fn like `#[tokio::test]` /
//! `#[crate::test]`, or a `#[cfg(test)]` module) are skipped — they are
//! throwaway fixtures that never deserialize untrusted input.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

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
        // Structs defined inside a test function or `#[cfg(test)]` module
        // are throwaway fixtures that never see untrusted input.
        if is_in_test_context(node, source_bytes) {
            return;
        }
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
        // `#[serde(transparent)]` structs delegate all (de)serialization
        // to their single inner field, so they have no field-name map of
        // their own — `deny_unknown_fields` is inert there.
        if attrs.iter().any(|a| has_transparent_attr(a)) {
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
        // `deny_unknown_fields` only affects structs deserialized from a
        // map of named fields. A tuple / newtype struct
        // (`struct Foo(T)`, body = `ordered_field_declaration_list`) or a
        // unit struct (`struct Foo;`, no body) delegates to the inner
        // type's deserializer and has no field-name map — the attribute is
        // inert there, so flagging it is a false positive.
        if !has_named_fields(node) {
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
    // Match `#[derive(..., Deserialize, ...)]` only when a derive entry's
    // final path segment is exactly `Deserialize` (so `serde::Deserialize`
    // counts). A custom derive that merely *contains* the substring, such
    // as `ConfigDeserialize`, is a different trait and must not trigger
    // the requirement.
    derive_paths(attr_text).any(|path| final_segment(path) == "Deserialize")
}

/// Yield each derive entry inside `#[derive(...)]` as a trimmed path
/// string (e.g. `Deserialize`, `serde::Deserialize`). Returns nothing
/// when the text is not a derive attribute.
fn derive_paths(attr_text: &str) -> impl Iterator<Item = &str> {
    attr_text
        .split_once("derive(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside)
        .into_iter()
        .flat_map(|inside| inside.split(','))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
}

/// The last `::`-separated segment of a path token (`serde::Deserialize`
/// -> `Deserialize`, `Deserialize` -> `Deserialize`).
fn final_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path).trim()
}

fn has_deny_unknown_fields(attr_text: &str) -> bool {
    attr_text.contains("deny_unknown_fields")
}

/// True for a `#[serde(...)]` attribute whose argument list contains the
/// `transparent` option. Scoped to the `serde(` argument list so an
/// unrelated attribute (e.g. `#[cfg(feature = "transparent")]`) does not
/// match.
fn has_transparent_attr(attr_text: &str) -> bool {
    attr_text
        .split_once("serde(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .is_some_and(|(inside, _)| {
            inside
                .split(',')
                .map(str::trim)
                .any(|opt| opt == "transparent")
        })
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

/// True only for a struct with a named-field body
/// (`field_declaration_list`). Tuple / newtype structs
/// (`ordered_field_declaration_list`) and unit structs (no body) return
/// false — `deny_unknown_fields` is inert on them.
fn has_named_fields(struct_node: tree_sitter::Node) -> bool {
    struct_node
        .child_by_field_name("body")
        .is_some_and(|body| body.kind() == "field_declaration_list")
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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
                          extra: std::collections::HashMap<String, toml::Value>,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "false positive: struct with flatten field can't have deny_unknown_fields"
        );
    }

    #[test]
    fn allows_transparent_newtype_struct() {
        // sqlx's `#[serde(transparent)] pub struct Json<T>(pub T);` —
        // a transparent newtype delegates all (de)serialization to its
        // inner field, so `deny_unknown_fields` is a no-op. (Closes #3879)
        let source = "#[derive(Deserialize)]\n#[serde(transparent)]\npub struct Json<T>(pub T);";
        assert!(
            run_on(source).is_empty(),
            "FP: transparent newtype flagged despite deny_unknown_fields being inert"
        );
    }

    #[test]
    fn allows_transparent_named_field_struct() {
        // A transparent struct with a single *named* field is not caught
        // by the tuple/newtype guard — the transparent exemption must
        // still skip it because field handling is delegated to `inner`.
        let source =
            "#[derive(Deserialize)]\n#[serde(transparent)]\nstruct Wrapper { inner: u32 }";
        assert!(
            run_on(source).is_empty(),
            "FP: transparent named-field struct flagged despite deny_unknown_fields being inert"
        );
    }

    #[test]
    fn flags_despite_unrelated_transparent_mention() {
        // `transparent` outside a `serde(...)` arg list (here a cfg
        // feature gate) must NOT trigger the exemption.
        let source = "#[derive(Deserialize)]\n#[cfg(feature = \"transparent\")]\nstruct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: `transparent` is a feature name, not serde(transparent)"
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
    fn allows_custom_derive_containing_deserialize_substring() {
        // `ConfigDeserialize` (alacritty's own proc-macro) is NOT serde's
        // `Deserialize` — it must not trigger the requirement even though
        // its name contains the substring "Deserialize". (Closes #1476)
        let source = "#[derive(ConfigDeserialize, Serialize, Debug, Clone, PartialEq, Eq)]\n\
                      pub struct Font {\n\
                          pub use_thin_strokes: bool,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: custom derive `ConfigDeserialize` flagged as serde Deserialize"
        );
    }

    #[test]
    fn flags_fully_qualified_serde_deserialize() {
        // `serde::Deserialize` — final path segment is exactly
        // `Deserialize`, so it must still fire without deny_unknown_fields.
        let source = "#[derive(serde::Deserialize)]\nstruct Config { rate: u32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_struct_inside_path_qualified_test_fn() {
        // axum's `#[crate::test]` fixtures (json.rs) — a throwaway
        // `Deserialize` struct inside the test fn must not be flagged.
        // (Closes #1259)
        let source = "#[crate::test]\n\
                      async fn deserialize_body() {\n\
                          #[derive(Debug, Deserialize)]\n\
                          struct Input { foo: String }\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: Deserialize fixture inside #[crate::test] fn flagged"
        );
    }

    #[test]
    fn allows_struct_inside_tokio_test_fn() {
        let source = "#[tokio::test]\n\
                      async fn roundtrip() {\n\
                          #[derive(Deserialize)]\n\
                          struct Foo { bar: u32 }\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: Deserialize fixture inside #[tokio::test] fn flagged"
        );
    }

    #[test]
    fn allows_struct_inside_cfg_test_module() {
        let source = "#[cfg(test)]\n\
                      mod tests {\n\
                          #[derive(Deserialize)]\n\
                          struct Input { foo: String }\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: Deserialize struct inside #[cfg(test)] module flagged"
        );
    }

    #[test]
    fn still_flags_production_struct_outside_test_context() {
        // Negative space: a non-test `Deserialize` struct missing
        // `deny_unknown_fields` is still flagged.
        let source = "#[derive(Deserialize)]\nstruct Input { foo: String }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_newtype_struct() {
        // A newtype struct deserializes via the inner type's deserializer —
        // there is no field-name map, so `deny_unknown_fields` is inert and
        // flagging it is a false positive (bevy `EntityHashSet`). (Closes #3935)
        let source = "#[derive(Deserialize)]\npub struct EntityHashSet(HashSet<Entity>);";
        assert!(
            run_on(source).is_empty(),
            "FP: newtype struct flagged despite deny_unknown_fields being inert"
        );
    }

    #[test]
    fn allows_multi_field_tuple_struct() {
        let source = "#[derive(Deserialize)]\nstruct Pair(u32, u32);";
        assert!(
            run_on(source).is_empty(),
            "FP: tuple struct flagged despite deny_unknown_fields being inert"
        );
    }

    #[test]
    fn allows_unit_struct() {
        let source = "#[derive(Deserialize)]\nstruct Unit;";
        assert!(
            run_on(source).is_empty(),
            "FP: unit struct flagged despite deny_unknown_fields being inert"
        );
    }

    #[test]
    fn flags_despite_incidental_api_mention() {
        // "external api" alone does NOT trigger the exemption — must be "external api response"
        let source = "// fetches data from external api of payment provider\n#[derive(Deserialize)]\nstruct PaymentData { amount: u64 }";
        assert_eq!(run_on(source).len(), 1, "should still flag: comment mentions external api but not 'external api response'");
    }
}
