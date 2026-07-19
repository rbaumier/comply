//! rust-serde-deny-unknown-fields backend.
//!
//! For every named-field `struct_item` with a
//! `#[derive(..., Deserialize, ...)]` attribute, scan the preceding
//! attribute siblings for `#[serde(deny_unknown_fields)]`. If absent,
//! flag the struct.
//!
//! A `Deserialize` derive whose derive list also contains `Archive` is
//! rkyv's, not serde's (`Archive` is rkyv-exclusive; rkyv re-exports a
//! `Deserialize` derive under the same bare name), so it is not flagged.
//!
//! A struct carrying a `#[sats(...)]` helper attribute is a SpacetimeDB SATS
//! type: its `Deserialize` derive is the SATS algebraic-type-system
//! deserializer (`spacetimedb_sats::de::Deserialize`), not serde's, so
//! `deny_unknown_fields` is inert and it is not flagged. This mirrors the rkyv
//! `Archive` exclusion — a co-located marker identifying the `Deserialize` as a
//! non-serde framework's.
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
//! **Exception:** a struct that is the *target* of a `#[serde(flatten)]`
//! field on another struct in the same file is NOT flagged. serde forbids
//! `deny_unknown_fields` on a flatten target too: when it is flattened into
//! a parent alongside a sibling flattened struct, the two share one field
//! map, so `deny_unknown_fields` on the target would reject the sibling's
//! fields as unknown and break the parent's deserialization. The target is
//! resolved by same-file type-name identity (a field typed `Key` or
//! `crate::x::Key` flattens the struct named `Key`).
//!
//! **Exception:** a `#[serde(transparent)]` struct is NOT flagged. It
//! delegates all (de)serialization to its single inner field and has no
//! field-name map of its own, so `deny_unknown_fields` is a no-op there.
//!
//! **Exception:** a `#[non_exhaustive]` struct is NOT flagged. It is the
//! explicit forward-compatibility opt-in — the struct may gain fields in
//! future versions — which directly contradicts `deny_unknown_fields`'s
//! rejection of any not-yet-declared field.
//!
//! **Exception:** structs defined inside a test context (a `#[test]`
//! function, a path-qualified test fn like `#[tokio::test]` /
//! `#[crate::test]`, or a `#[cfg(test)]` module) are skipped — they are
//! throwaway fixtures that never deserialize untrusted input.
//!
//! **Exception:** structs in a cargo-fuzz target (a file under a
//! `fuzz_targets/` directory) are skipped. These harnesses deliberately
//! feed the deserializer random/malformed bytes; `deny_unknown_fields`
//! would reject inputs before the fuzz target can exercise the serde
//! code paths, defeating the fuzzer's purpose.
//!
//! **Exception:** a struct defined as a local item inside a function body
//! (any `function_item` ancestor, including an impl-method body) is skipped.
//! Such locals are ad-hoc partial parsers that intentionally capture only
//! the fields the caller needs from an external format; `deny_unknown_fields`
//! would reject every real input carrying the fields they deliberately
//! ignore. They are never public-API types needing strict field validation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{enclosing_fn, is_in_test_context};

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

        // cargo-fuzz targets deliberately deserialize random/malformed bytes;
        // `deny_unknown_fields` would reject inputs before the fuzz target can
        // exercise the serde code paths.
        if crate::rules::path_utils::is_fuzz_targets_path(ctx.path) {
            return;
        }

        let source_bytes = ctx.source.as_bytes();
        // Structs defined inside a test function or `#[cfg(test)]` module
        // are throwaway fixtures that never see untrusted input.
        if is_in_test_context(node, source_bytes) {
            return;
        }
        // A struct defined as a local item inside a function body (any
        // `function_item` ancestor, including an impl-method body) is an
        // ad-hoc partial parser of an external format, capturing only the
        // fields the caller needs. `deny_unknown_fields` would reject every
        // real input carrying the extra fields the parser deliberately
        // ignores, so these locals are never flagged.
        if enclosing_fn(node).is_some() {
            return;
        }
        let attrs = collect_preceding_attrs(node, source_bytes);
        if !attrs.iter().any(|a| derives_deserialize(a)) {
            return;
        }
        // A struct carrying a `#[sats(...)]` helper attribute is a SpacetimeDB
        // SATS type: its `Deserialize` derive is the SATS algebraic-type
        // deserializer (`spacetimedb_sats::de::Deserialize`), not serde's, so
        // `deny_unknown_fields` is meaningless there. Symmetric to the rkyv
        // `Archive` co-derive exclusion in `derives_deserialize` — a co-located
        // marker identifying the `Deserialize` as a non-serde framework's.
        if attrs.iter().any(|a| has_sats_attr(a)) {
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
        // `#[non_exhaustive]` is the explicit forward-compatibility opt-in: the
        // struct may gain fields in future versions. `deny_unknown_fields` has
        // the opposite semantics (reject any not-yet-declared field), so the two
        // are contradictory — a `#[non_exhaustive]` Deserialize struct must NOT
        // use deny_unknown_fields.
        if attrs.iter().any(|a| has_non_exhaustive_attr(a)) {
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
        // A struct that is itself the type of a `#[serde(flatten)]` field on
        // another struct in this file is a flatten *target*. serde forbids
        // `deny_unknown_fields` on a flatten target just as on a flatten source:
        // when several structs are flattened into one parent, enabling it on a
        // target makes it reject its siblings' fields as unknown and breaks the
        // parent's deserialization. Symmetric to `has_flatten_field`.
        if let Some(root) = source_file_root(node)
            && source_file_flattens_type_named(root, name, source_bytes)
        {
            return;
        }
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
            severity: Severity::Error,
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
    //
    // A derive list that also derives `Archive` is rkyv's, not serde's:
    // `Archive` is rkyv-exclusive, and rkyv re-exports a `Deserialize`
    // derive under that same bare name (`use rkyv::{Archive, Deserialize}`).
    // Its `Deserialize` is a zero-copy framework trait unrelated to serde
    // field parsing, so `deny_unknown_fields` is meaningless there. Scope
    // the check to the *same* derive list so a separate
    // `#[derive(serde::Deserialize)]` still fires.
    let paths: Vec<&str> = derive_paths(attr_text).collect();
    if paths.iter().any(|p| final_segment(p) == "Archive") {
        return false;
    }
    paths.iter().any(|path| final_segment(path) == "Deserialize")
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

/// True for the bare `#[non_exhaustive]` attribute. Matches on the
/// attribute's meta path being exactly `non_exhaustive` (after stripping
/// the `#[` / `]` delimiters and surrounding whitespace), so an unrelated
/// occurrence of the word — e.g. `#[serde(rename = "non_exhaustive")]` —
/// does not match.
fn has_non_exhaustive_attr(attr_text: &str) -> bool {
    attr_text
        .strip_prefix("#[")
        .and_then(|rest| rest.strip_suffix(']'))
        .is_some_and(|meta| meta.trim() == "non_exhaustive")
}

/// True for a struct-level `#[sats(...)]` (or bare `#[sats]`) helper attribute —
/// the marker the SpacetimeDB SATS derive macros (`spacetimedb_sats::de::Deserialize`
/// / `ser::Serialize`) attach. Matches on the attribute's leading path segment
/// being exactly `sats` (after stripping the `#[ … ]` framing and reading the
/// path before any `( … )` arguments), so an unrelated attribute whose argument
/// list merely contains the word — e.g. `#[serde(rename = "sats")]` — does not
/// match.
fn has_sats_attr(attr_text: &str) -> bool {
    let inner = attr_text
        .trim()
        .strip_prefix("#[")
        .and_then(|s| s.strip_suffix(']'))
        .map(str::trim)
        .unwrap_or("");
    let path = inner
        .split(|c: char| c == '=' || c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("");
    path == "sats"
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

/// True if any field inside the struct body carries a `#[serde(flatten)]`
/// attribute — i.e. the struct is a flatten *source*.
fn has_flatten_field(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = struct_node.child_by_field_name("body") else {
        return false;
    };
    if body.kind() != "field_declaration_list" {
        return false;
    }
    let mut cursor = body.walk();
    body.children(&mut cursor)
        .any(|field| field.kind() == "field_declaration" && field_has_flatten_attr(field, source))
}

/// True if `field` (a `field_declaration`) has a preceding `#[serde(flatten)]`
/// attribute. Interleaved comments are skipped; the scan stops at the first
/// non-attribute, non-comment sibling.
fn field_has_flatten_attr(field: tree_sitter::Node, source: &[u8]) -> bool {
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
    false
}

/// Walk up from `node` to the enclosing `source_file` root.
fn source_file_root(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = Some(node);
    while let Some(n) = current {
        if n.kind() == "source_file" {
            return Some(n);
        }
        current = n.parent();
    }
    None
}

/// True if any `struct_item` in the file has a `#[serde(flatten)]` field whose
/// type resolves (final path segment) to `name` — i.e. the struct named `name`
/// is a flatten *target*. Descends the whole subtree so a struct nested in a
/// `mod` is found, mirroring the same-file enum-definition walks.
///
/// This is a full-file walk, invoked once per otherwise-flaggable struct (the
/// call site is guarded by every cheaper exemption first), not a bounded
/// subtree scan; the candidate set per file is small, so it is not a hot path.
fn source_file_flattens_type_named(
    source_file: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "struct_item" && struct_flattens_type_named(node, name, source) {
            return true;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `struct_node` has a `#[serde(flatten)]` field whose type's final
/// path segment is `name`.
fn struct_flattens_type_named(struct_node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let Some(body) = struct_node.child_by_field_name("body") else {
        return false;
    };
    if body.kind() != "field_declaration_list" {
        return false;
    }
    let mut cursor = body.walk();
    body.children(&mut cursor).any(|field| {
        field.kind() == "field_declaration"
            && field_has_flatten_attr(field, source)
            && field
                .child_by_field_name("type")
                .and_then(|ty| field_type_final_segment(ty, source))
                == Some(name)
    })
}

/// The final path-segment identifier of a field type, ignoring generic
/// arguments: `Key` -> `Key`, `crate::model::Key` -> `Key`, `Key<T>` -> `Key`.
/// Returns `None` for shapes with no single leading path (references, tuples,
/// `dyn`/`impl`, …), which never name a flatten-target struct.
fn field_type_final_segment<'a>(type_node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match type_node.kind() {
        "type_identifier" => type_node.utf8_text(source).ok(),
        "scoped_type_identifier" => type_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok()),
        "generic_type" => type_node
            .child_by_field_name("type")
            .and_then(|base| field_type_final_segment(base, source)),
        _ => None,
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
    fn allows_flatten_target_structs() {
        // qdrant clock_map.rs: `Key` and `Clock` are each a `#[serde(flatten)]`
        // target of `KeyClockHelper`. serde forbids `deny_unknown_fields` on a
        // flatten target, so neither may be flagged; `KeyClockHelper` itself is
        // exempt via the existing flatten-source path. (Closes #7681)
        let source = "#[derive(Copy, Clone, Deserialize, Serialize)]\n\
                      pub struct Key { peer_id: PeerId, clock_id: u32 }\n\
                      #[derive(Copy, Clone, Deserialize, Serialize)]\n\
                      struct Clock { current_tick: u64, token: ClockToken }\n\
                      #[derive(Copy, Clone, Deserialize, Serialize)]\n\
                      struct KeyClockHelper {\n\
                          #[serde(flatten)]\n\
                          key: Key,\n\
                          #[serde(flatten)]\n\
                          clock: Clock,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: flatten-target structs (Key, Clock) or the flatten-source helper flagged"
        );
    }

    #[test]
    fn flags_non_target_but_exempts_flatten_target_in_same_file() {
        // `Inner` is a flatten target (exempt); `Plain` is a normal Deserialize
        // struct that is NOT flattened anywhere — it must still be flagged.
        // Proves the exemption is scoped to referenced type names, not a blanket
        // pass on every struct in a file that happens to use flatten somewhere.
        let source = "#[derive(Deserialize)]\n\
                      struct Inner { a: u32 }\n\
                      #[derive(Deserialize)]\n\
                      struct Wrapper {\n\
                          #[serde(flatten)]\n\
                          inner: Inner,\n\
                      }\n\
                      #[derive(Deserialize)]\n\
                      struct Plain { b: u32 }";
        let diags = run_on(source);
        assert_eq!(diags.len(), 1, "only the non-target `Plain` struct should flag");
        assert!(
            diags[0].message.contains("`Plain`"),
            "the flagged struct must be `Plain`, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_flatten_target_referenced_by_qualified_path() {
        // The flatten field's type is a path (`crate::model::Key`); resolving to
        // its final segment `Key` must still exempt the same-file `Key` struct.
        let source = "#[derive(Deserialize)]\n\
                      struct Key { peer_id: u32 }\n\
                      #[derive(Deserialize)]\n\
                      struct Helper {\n\
                          #[serde(flatten)]\n\
                          key: crate::model::Key,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: flatten target referenced by a qualified path not exempted"
        );
    }

    #[test]
    fn allows_flatten_target_referenced_by_generic_type() {
        // A flatten field typed `Key<u32>` resolves to its base segment `Key`
        // (generic args ignored), so the same-file generic `Key<T>` struct is
        // exempted.
        let source = "#[derive(Deserialize)]\n\
                      struct Key<T> { peer_id: T }\n\
                      #[derive(Deserialize)]\n\
                      struct Helper {\n\
                          #[serde(flatten)]\n\
                          key: Key<u32>,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: flatten target referenced by a generic type not exempted"
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
    fn allows_non_exhaustive_struct() {
        // `#[non_exhaustive]` is the explicit forward-compat opt-in: the
        // struct may gain fields in future versions. `deny_unknown_fields`
        // has the opposite semantics, so the two are contradictory. (hyperium
        // /tonic BootstrapConfig — closes #4445)
        let source = "#[derive(Debug, Clone, Deserialize)]\n\
                      #[non_exhaustive]\n\
                      pub struct BootstrapConfig { pub a: Vec<u8>, pub b: u32 }";
        assert!(
            run_on(source).is_empty(),
            "FP: #[non_exhaustive] struct flagged despite being forward-compat opt-in"
        );
    }

    #[test]
    fn allows_non_exhaustive_struct_attr_order_swapped() {
        // Same exemption when `#[non_exhaustive]` precedes the derive.
        let source = "#[non_exhaustive]\n\
                      #[derive(Debug, Clone, Deserialize)]\n\
                      pub struct BootstrapConfig { pub a: Vec<u8>, pub b: u32 }";
        assert!(
            run_on(source).is_empty(),
            "FP: #[non_exhaustive] struct flagged despite being forward-compat opt-in"
        );
    }

    #[test]
    fn allows_non_exhaustive_struct_with_field_serde_attrs() {
        // Verbatim issue shape: pub(crate) fields carrying `#[serde(default)]`.
        // The struct-level `#[non_exhaustive]` exemption must hold regardless of
        // field-level serde attributes. (hyperium/tonic BootstrapConfig)
        let source = "#[derive(Debug, Clone, Deserialize)]\n\
                      #[non_exhaustive]\n\
                      pub struct BootstrapConfig {\n\
                          pub(crate) xds_servers: Vec<XdsServerConfig>,\n\
                          #[serde(default)]\n\
                          pub(crate) node: NodeConfig,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: #[non_exhaustive] struct flagged despite being forward-compat opt-in"
        );
    }

    #[test]
    fn flags_despite_unrelated_non_exhaustive_mention() {
        // A serde rename to the literal string "non_exhaustive" is NOT the
        // bare `#[non_exhaustive]` attribute and must NOT trigger the exemption.
        let source = "#[derive(Deserialize)]\n\
                      struct Config {\n\
                          #[serde(rename = \"non_exhaustive\")]\n\
                          rate: u32,\n\
                      }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: a serde rename to \"non_exhaustive\" is not the attribute"
        );
    }

    #[test]
    fn allows_fuzz_target_struct() {
        // A struct in a cargo-fuzz target deriving `Arbitrary` deliberately
        // deserializes random/malformed bytes — `deny_unknown_fields` would
        // reject inputs before the fuzzer can exercise serde. (rhaiscript/rhai
        // fuzz/fuzz_targets/fuzz_serde.rs — closes #4793)
        let source = "#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize)]\n\
                      struct AllTypes { _bool: bool, _str: String }";
        let diags = crate::rules::test_helpers::run_rule(
            &Check,
            source,
            "fuzz/fuzz_targets/fuzz_serde.rs",
        );
        assert!(
            diags.is_empty(),
            "FP: fuzz-target struct flagged despite living under fuzz_targets/"
        );
    }

    #[test]
    fn still_flags_deserialize_struct_outside_fuzz_targets() {
        // Negative space: the same struct shape outside a fuzz_targets/ path is
        // still flagged — the exemption is scoped to the fuzz directory.
        let source = "#[derive(Debug, Clone, Deserialize)]\n\
                      struct AllTypes { _bool: bool, _str: String }";
        let diags = crate::rules::test_helpers::run_rule(&Check, source, "src/config.rs");
        assert_eq!(
            diags.len(),
            1,
            "should still flag a non-fuzz Deserialize struct missing deny_unknown_fields"
        );
    }

    #[test]
    fn flags_despite_incidental_api_mention() {
        // "external api" alone does NOT trigger the exemption — must be "external api response"
        let source = "// fetches data from external api of payment provider\n#[derive(Deserialize)]\nstruct PaymentData { amount: u64 }";
        assert_eq!(run_on(source).len(), 1, "should still flag: comment mentions external api but not 'external api response'");
    }

    #[test]
    fn allows_rkyv_deserialize_via_cfg_attr_bare_name() {
        // chrono `use rkyv::{Archive, Deserialize, Serialize}` then a
        // feature-gated `derive(Archive, Deserialize, Serialize)` — the bare
        // `Deserialize` is rkyv's, not serde's. `Archive` in the same derive
        // list is the rkyv signal. (Closes #4995)
        let source = "#[derive(Clone)]\n\
                      #[cfg_attr(\n\
                          any(feature = \"rkyv\", feature = \"rkyv-16\", feature = \"rkyv-32\", feature = \"rkyv-64\"),\n\
                          derive(Archive, Deserialize, Serialize),\n\
                          archive(compare(PartialEq, PartialOrd))\n\
                      )]\n\
                      pub struct DateTime { datetime: NaiveDateTime, offset: i32 }";
        assert!(
            run_on(source).is_empty(),
            "FP: rkyv `Deserialize` (co-derived with `Archive`) flagged as serde"
        );
    }

    #[test]
    fn allows_rkyv_deserialize_plain_derive() {
        // Even without cfg_attr, a `derive(Archive, Deserialize)` is rkyv's.
        let source = "#[derive(Archive, Deserialize, Serialize)]\nstruct Pos { x: i32, y: i32 }";
        assert!(
            run_on(source).is_empty(),
            "FP: rkyv `Deserialize` co-derived with `Archive` flagged as serde"
        );
    }

    #[test]
    fn still_flags_serde_deserialize_without_archive() {
        // Negative space: a genuine serde `Deserialize` (no `Archive` in the
        // derive list) missing `deny_unknown_fields` is still flagged.
        let source = "use serde::Deserialize;\n#[derive(Deserialize)]\nstruct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: serde Deserialize without Archive is the real target"
        );
    }

    #[test]
    fn allows_local_struct_inside_function_body() {
        // tokei `fn parse_jupyter` — `Jupyter` is a local item inside a
        // non-test function body, an ad-hoc partial parser of the Jupyter
        // notebook format that intentionally ignores the file's many other
        // fields. `deny_unknown_fields` would make every real notebook fail
        // to parse. (Closes #6578)
        let source = "fn parse_jupyter(json: &[u8]) -> Option<CodeStats> {\n\
                      #[derive(Deserialize)]\n\
                      struct Jupyter {\n\
                          cells: Vec<JupyterCell>,\n\
                          metadata: JupyterMetadata,\n\
                      }\n\
                      let jupyter: Jupyter = serde_json::from_slice(json).ok()?;\n\
                      None\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: local Deserialize struct inside a fn body flagged"
        );
    }

    #[test]
    fn allows_local_struct_inside_impl_method_body() {
        // A struct local to an impl-method body is still a local partial
        // parser — the method is a `function_item`, so it is exempted.
        let source = "impl Parser {\n\
                      fn parse(&self, json: &[u8]) -> Option<()> {\n\
                      #[derive(Deserialize)]\n\
                      struct Cell { source: Vec<String> }\n\
                      None\n\
                      }\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "FP: local Deserialize struct inside an impl-method body flagged"
        );
    }

    #[test]
    fn still_flags_top_level_struct_outside_any_fn() {
        // Negative control: a module-level `Deserialize` struct (no
        // `function_item` ancestor) missing `deny_unknown_fields` still flags.
        let source = "#[derive(Deserialize)]\nstruct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "top-level Deserialize struct must still flag"
        );
    }

    #[test]
    fn still_flags_struct_inside_plain_module() {
        // Negative control: a struct inside `mod m { ... }` but not inside any
        // function body still flags — `mod` is not a `function_item`.
        let source = "mod m {\n\
                      #[derive(Deserialize)]\n\
                      struct Config { rate: u32 }\n\
                      }";
        assert_eq!(
            run_on(source).len(),
            1,
            "module-level Deserialize struct must still flag"
        );
    }

    #[test]
    fn does_not_flag_combined_rkyv_and_serde_in_one_derive_list() {
        // Boundary: a single derive list carrying both `Archive` (rkyv) and a
        // disambiguated serde `Deserialize` is deliberately NOT flagged — the
        // `Archive` signal wins to keep the common rkyv FP suppressed. This
        // (rare) co-derive form is an accepted trade-off; a separate serde
        // derive attribute still fires (see flags_fully_qualified_serde_deserialize).
        let source =
            "#[derive(rkyv::Archive, serde::Deserialize)]\nstruct Pos { x: i32, y: i32 }";
        assert!(
            run_on(source).is_empty(),
            "accepted trade-off: Archive co-derive suppresses the serde warning in one list"
        );
    }

    #[test]
    fn allows_sats_deserialize_struct() {
        // SpacetimeDB SATS type (crates/sats/src/timestamp.rs): the `Deserialize`
        // derive is `crate::de::Deserialize` (SATS's algebraic-type deserializer),
        // not serde's — the file has no serde at all. The `#[sats(crate = crate)]`
        // helper attribute marks it as a SATS type, so `deny_unknown_fields` is
        // inert. (Closes #7829)
        let source = "use crate::de::Deserialize;\n\
                      #[derive(Eq, PartialEq, Copy, Clone, Hash, Serialize, Deserialize, Debug)]\n\
                      #[sats(crate = crate)]\n\
                      pub struct Timestamp { micros: i64 }";
        assert!(
            run_on(source).is_empty(),
            "FP: SATS `#[sats(...)]` Deserialize (non-serde) flagged"
        );
    }

    #[test]
    fn allows_sats_deserialize_bare_attr() {
        // The `#[sats]` helper attribute (no arguments) still marks a SATS type.
        let source = "#[derive(Serialize, Deserialize)]\n\
                      #[sats]\n\
                      struct TimeDuration { micros: i64 }";
        assert!(
            run_on(source).is_empty(),
            "FP: SATS `#[sats]` Deserialize (non-serde) flagged"
        );
    }

    #[test]
    fn flags_despite_unrelated_sats_mention() {
        // `sats` appearing inside a struct-level `#[serde(...)]` argument list is
        // NOT the `#[sats(...)]` helper attribute (its path is `serde`, not
        // `sats`) and must not trigger the SATS exemption. The mention is a
        // struct-level attribute so `has_sats_attr` is actually exercised on it.
        let source = "#[derive(Deserialize)]\n\
                      #[serde(rename_all = \"sats\")]\n\
                      struct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: `sats` in a serde arg list is not the #[sats] attribute"
        );
    }
}
