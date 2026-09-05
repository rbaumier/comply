//! rust-serde-deny-unknown-fields backend.
//!
//! For every named-field `struct_item` with a
//! `#[derive(..., Deserialize, ...)]` attribute, scan the preceding
//! attribute siblings for `#[serde(deny_unknown_fields)]`. If absent,
//! flag the struct.
//!
//! Only *serde's* container option discharges the requirement.
//! [`has_attribute_option`] matches it structurally: the attribute path must be
//! `serde`, and `deny_unknown_fields` must name a meta item of its argument
//! list. Another crate's helper attribute can spell the same word.
//! `#[schemars(deny_unknown_fields)]` configures a generated JSON Schema
//! document. It never reaches serde's `Deserialize` impl, so it does not count.
//!
//! The `#[cfg_attr(<predicate>, serde(deny_unknown_fields))]` form does count.
//! The author declared serde's option. Which build configurations enable it is
//! not decidable from the crate's own source: any consumer can pass
//! `--no-default-features` and switch a `default` feature off. Crates gate the
//! option deliberately, to be strict in one build and lenient in another.
//! `#[cfg_attr(test, serde(deny_unknown_fields))]` on a mirror of an external
//! tool's JSON output is the archetype. The test build detects upstream schema
//! drift; the shipped build tolerates it. Accepting the gated form therefore
//! gives up the typo detection this rule asks for, in the builds where the
//! predicate is false. That price buys a decidable question — *which attribute
//! is this* — in place of an undecidable one.
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
//! Every exception below is one statement: *the set of field names this
//! declaration accepts from input is not fixed at compile time, the declaration
//! has no field-name map of its own, or no field name of it is ever written by
//! hand*. `#[non_exhaustive]`, `#[serde(flatten)]` and a `#[cfg]`-gated field
//! are three spellings of the first; `transparent`, `from`/`try_from` and a
//! tuple body are the second; the opaque round-trip type is the third. A new
//! case belongs under that statement, not beside the list.
//!
//! Field *visibility* alone is not one of them. A `Deserialize` struct's wire
//! names are its field identifiers whatever their Rust visibility, and the
//! document is written outside Rust — a binary crate's config type is usually
//! private and is exactly what a user hand-writes and mistypes. It is only in
//! conjunction with reachability and a `Serialize` derive that it identifies a
//! document no human authors; see the opaque-round-trip exception below.
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
//! **Exception:** a `#[serde(from = "T")]` / `#[serde(try_from = "T")]`
//! struct is NOT flagged. serde builds its `Deserialize` impl by
//! deserializing `T` and converting, so the struct's own field names never
//! parse anything — the `transparent` situation, and serde rejects
//! combining either attribute with `deny_unknown_fields`.
//!
//! **Exception:** a struct with a `#[cfg(...)]`-gated field is NOT flagged.
//! `deny_unknown_fields` is whole-struct, so it would turn a feature
//! mismatch between the build that wrote a value and the build that reads
//! it into a hard deserialization failure, with no way to exempt the one
//! conditional key.
//!
//! **Exception:** an opaque round-trip type is NOT flagged — a struct another
//! crate can name (reachable from outside this one) but whose keys it cannot
//! (every named field non-`pub`), deriving `Serialize` alongside `Deserialize`.
//! Its serialized form has exactly one producer, the crate's own `Serialize`
//! impl on a value the crate built, so there is no hand-written key to mistype;
//! `deny_unknown_fields` would only freeze the private field names into a
//! compatibility surface, making a rename break every previously-saved value.
//! Each conjunct carries weight: a binary crate's config type is private and
//! hand-written, and a `Deserialize`-only reader gets its document from
//! somebody else whatever its field visibility.
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
use crate::rules::rust_helpers::{
    crate_has_external_consumers, enclosing_fn, has_attribute_option, has_outer_attribute_path,
    is_effectively_pub, is_pub, is_test_code,

};

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
        if is_test_code(node, source_bytes, ctx) {
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
        // Only serde's own container option discharges the requirement. Another
        // crate's helper attribute spelling the same word
        // (`#[schemars(deny_unknown_fields)]`) configures that crate's output,
        // never serde's `Deserialize` impl.
        if has_attribute_option(node, source_bytes, "serde", "deny_unknown_fields") {
            return;
        }
        // Structs with a `#[serde(flatten)]` field cannot have
        // `deny_unknown_fields` — the two are mutually exclusive.
        if has_flatten_field(node, source_bytes) {
            return;
        }
        // A `#[cfg(...)]`-gated field makes the accepted key set depend on how
        // the crate was compiled, not on the declaration. `deny_unknown_fields`
        // is whole-struct, so enabling it would make a value written by a build
        // that has the field fail to load in a build that does not — the same
        // version, two feature sets. There is no way to spell "reject unknown
        // keys except this one".
        if has_cfg_gated_field(node, source_bytes) {
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
        if has_attribute_option(node, source_bytes, "serde", "transparent") {
            return;
        }
        // `#[serde(from = "T")]` / `#[serde(try_from = "T")]` build the
        // `Deserialize` impl by deserializing `T` and converting, so this
        // struct's own field names never take part in parsing — the same
        // no-field-map-of-its-own situation as `transparent`. serde rejects
        // combining either with `deny_unknown_fields` outright.
        if has_attribute_option(node, source_bytes, "serde", "from")
            || has_attribute_option(node, source_bytes, "serde", "try_from")
        {
            return;
        }
        // ORM structs (Diesel Queryable / Selectable) deserialize from
        // internal query results, not user input — forward-compat is
        // more important than strict field validation.
        if has_orm_derive(&attrs) {
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
        // A struct another crate can name but whose keys it cannot — every field
        // non-`pub` — and which derives `Serialize` alongside `Deserialize` is an
        // opaque round-trip type: the crate's own impl is the only producer of
        // the serialized form, so there is no hand-written key to mistype, and
        // `deny_unknown_fields` would promote names the author kept private into
        // a compatibility surface.
        if is_opaque_round_trip_struct(node, ctx, &attrs) {
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

/// True when `attr_text` is a derive list naming `Serialize` — the marker that
/// the crate itself produces the serialized form, and not only reads one.
/// Matched on the derive entry's final path segment, as [`derives_deserialize`]
/// matches its own.
fn derives_serialize(attr_text: &str) -> bool {
    derive_paths(attr_text).any(|path| final_segment(path) == "Serialize")
}

/// True for a struct another crate can name but whose keys it cannot: reachable
/// from outside this crate, every named field non-`pub`, and `Serialize`
/// derived alongside `Deserialize`.
///
/// Each conjunct answers one half of "who writes the document". Effective
/// publicity ([`is_effectively_pub`] plus [`crate_has_external_consumers`]) says
/// consumers hold values of the type; the absence of bare `pub` on every field
/// says they cannot learn a single key from the API or from rustdoc, which
/// renders the body as `/* private fields */`; the `Serialize` derive names the
/// only producer of the serialized form, the crate's own impl round-tripping
/// values it built. Together there is no hand-written key to mistype, and
/// `deny_unknown_fields` would freeze names the author kept private into a
/// compatibility surface — renaming one would stop old data from loading.
///
/// Drop any conjunct and the reasoning fails on a real shape. Without the
/// reachability half, a binary crate's config type — private by default and
/// hand-written by its user — is exactly what the rule exists for. Without the
/// `Serialize` half, a `Deserialize`-only reader gets its document from someone
/// else whatever its field visibility.
fn is_opaque_round_trip_struct(
    struct_node: tree_sitter::Node,
    ctx: &CheckCtx,
    attrs: &[String],
) -> bool {
    let source = ctx.source.as_bytes();
    if !is_effectively_pub(struct_node, source, ctx.path)
        || !crate_has_external_consumers(ctx.project, ctx.path)
        || !attrs.iter().any(|a| derives_serialize(a))
    {
        return false;
    }
    named_fields(struct_node).is_some_and(|fields| {
        !fields.is_empty() && fields.iter().all(|field| !is_pub(*field, source))
    })
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
    body.children(&mut cursor).any(|field| {
        field.kind() == "field_declaration"
            && has_attribute_option(field, source, "serde", "flatten")
    })
}

/// True if any field of the struct carries a `#[cfg(...)]` attribute, i.e. the
/// field set is decided at compile time rather than by the declaration.
///
/// Keyed on the attribute *path* being exactly `cfg`, read from the AST, so
/// `#[cfg_attr(feature = "serde", serde(skip_serializing_if = "…"))]` — a serde
/// helper applied conditionally, not a conditional field — does not match. Which
/// predicate the `cfg` names is irrelevant: `feature`, `target_os` and
/// `debug_assertions` all make the field's presence a property of the build.
fn has_cfg_gated_field(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    named_fields(struct_node).is_some_and(|fields| {
        fields
            .into_iter()
            .any(|field| has_outer_attribute_path(field, source, &["cfg"]))
    })
}

/// The `field_declaration` children of a named-field struct body, or `None` for
/// a tuple / newtype / unit struct that has no field-name map at all. Collected
/// eagerly because the `TreeCursor` the traversal needs cannot outlive the call.
fn named_fields<'tree>(
    struct_node: tree_sitter::Node<'tree>,
) -> Option<Vec<tree_sitter::Node<'tree>>> {
    let body = struct_node.child_by_field_name("body")?;
    if body.kind() != "field_declaration_list" {
        return None;
    }
    let mut cursor = body.walk();
    Some(
        body.children(&mut cursor)
            .filter(|child| child.kind() == "field_declaration")
            .collect(),
    )
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
            && has_attribute_option(field, source, "serde", "flatten")
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

    /// Run the rule on a file of an ordinary library crate. The
    /// opaque-round-trip exception asks whether another crate can name the type,
    /// which only a real manifest and file layout can answer.
    fn run_in_lib_crate(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_in_crate(
            &Check,
            crate::rules::test_helpers::LIB_CARGO_TOML,
            source,
        )
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
    fn repro_8438_external_api_response_phrase_no_longer_exempts() {
        // rbaumier/comply#8438 — the three-phrase comment allowlist is retired.
        // It exempted on what the author happened to write in prose, matched no
        // struct in any of the five measured corpora, and duplicated comply's own
        // `// comply-ignore: <rule-id> — <reason>` directive, which names the rule
        // and requires a justification.
        let source = "// external api response — version-compatible\n#[derive(Deserialize)]\nstruct GithubUser { login: String }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn repro_8438_versioned_protocol_phrase_no_longer_exempts() {
        let source = "// versioned protocol — accepts future fields\n#[derive(Deserialize)]\nstruct DapMessage { seq: i32 }";
        assert_eq!(run_on(source).len(), 1);
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
    fn flags_config_struct_whose_only_deny_is_schemars() {
        // starship `src/configs/rust.rs`: the config types every user edits by
        // hand in `starship.toml` carry `schemars(deny_unknown_fields)`, which
        // configures the generated JSON Schema and never reaches serde's
        // `Deserialize` impl. `symbl = "🦀 "` is still dropped silently.
        // (Closes #8361)
        let source = "#[derive(Clone, Deserialize, Serialize)]\n\
                      #[cfg_attr(\n\
                          feature = \"config-schema\",\n\
                          derive(schemars::JsonSchema),\n\
                          schemars(deny_unknown_fields)\n\
                      )]\n\
                      #[serde(default)]\n\
                      pub struct RustConfig<'a> {\n\
                          pub format: &'a str,\n\
                          pub symbol: &'a str,\n\
                          pub disabled: bool,\n\
                      }";
        let diags = run_on(source);
        assert_eq!(
            diags.len(),
            1,
            "schemars' option must not discharge serde's requirement"
        );
        assert!(
            diags[0].message.contains("`RustConfig`"),
            "the flagged struct must be `RustConfig`, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn flags_despite_unqualified_schemars_deny_unknown_fields() {
        // The same option applied directly, without a `cfg_attr` gate: the
        // attribute path is still `schemars`, so serde's `Deserialize` impl
        // accepts unknown keys.
        let source = "#[derive(Deserialize)]\n\
                      #[schemars(deny_unknown_fields)]\n\
                      struct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: `schemars(deny_unknown_fields)` is not serde's"
        );
    }

    #[test]
    fn flags_despite_third_party_schema_attr_deny_unknown_fields() {
        // utoipa's `#[schema(...)]` helper describes the OpenAPI document, not
        // serde's field map.
        let source = "#[derive(Deserialize)]\n\
                      #[schema(deny_unknown_fields)]\n\
                      struct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: `schema(deny_unknown_fields)` is not serde's"
        );
    }

    #[test]
    fn flags_despite_deny_unknown_fields_as_an_attribute_value() {
        // The word appearing as a *value* rather than an option: neither a
        // `#[doc = "…"]` string nor a serde rename enables the option.
        let source = "#[doc = \"set deny_unknown_fields on this one day\"]\n\
                      #[derive(Deserialize)]\n\
                      #[serde(rename = \"deny_unknown_fields\")]\n\
                      struct Config { rate: u32 }";
        assert_eq!(
            run_on(source).len(),
            1,
            "should still flag: the word is a value here, not a serde option"
        );
    }

    #[test]
    fn flags_struct_preceded_by_an_exempt_struct() {
        // The attribute scan walks preceding siblings and must stop at the first
        // one that is not an attribute or a comment. Otherwise the exemption of
        // one struct leaks onto every struct declared after it.
        let source = "#[derive(Deserialize)]\n\
                      #[serde(deny_unknown_fields)]\n\
                      struct Strict { rate: u32 }\n\
                      #[derive(Deserialize)]\n\
                      struct Loose { rate: u32 }";
        let findings = run_on(source);
        assert_eq!(
            findings.len(),
            1,
            "the exemption on `Strict` must not carry over to `Loose`"
        );
        assert!(
            findings[0].message.contains("`Loose`"),
            "the flagged struct should be `Loose`, got `{}`",
            findings[0].message
        );
    }

    #[test]
    fn allows_cfg_attr_gated_serde_deny_unknown_fields() {
        // Real crates gate the option to be strict in one build and
        // forward-compatible in another. The author declared serde's option; the
        // build configurations that enable it are the crate's call, not the
        // rule's. Both predicate spellings are locked because a `cfg` predicate
        // and a `feature` predicate parse into different token shapes.
        let sources = [
            // taiki-e/cargo-llvm-cov `src/json.rs`: a mirror of `llvm-cov
            // export`'s JSON, strict under `test` to detect upstream schema
            // drift.
            "#[derive(Deserialize)]\n\
             #[cfg_attr(test, serde(deny_unknown_fields))]\n\
             pub struct LlvmCovJsonExport { pub data: Vec<Export> }",
            // cobalt-org/cobalt.rs `crates/config/src/config.rs`: strict under
            // the `unstable` feature, forward-compatible otherwise.
            "#[derive(Deserialize)]\n\
             #[cfg_attr(feature = \"unstable\", serde(deny_unknown_fields))]\n\
             pub struct Config { pub source: String }",
        ];
        for source in sources {
            assert!(
                run_on(source).is_empty(),
                "FP: a cfg_attr-gated serde(deny_unknown_fields) declaration flagged in `{source}`"
            );
        }
    }

    #[test]
    fn allows_cfg_attr_gated_serde_transparent() {
        // Optional-serde crates gate the whole serde surface on one feature; the
        // `transparent` option is then declared through the same `cfg_attr`.
        let source = "#[derive(Deserialize)]\n\
                      #[cfg_attr(feature = \"serde\", serde(transparent))]\n\
                      struct Wrapper { inner: u32 }";
        assert!(
            run_on(source).is_empty(),
            "FP: cfg_attr-gated serde(transparent) struct flagged"
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

    #[test]
    fn repro_8075_serde_try_from_container_not_flagged() {
        // rbaumier/comply#8075 — `#[serde(try_from = "T")]` (oxc's `BabelPresets`)
        // deserializes `T` and converts, so this struct's field names never parse
        // input. serde also rejects pairing it with `deny_unknown_fields`.
        let source = "#[derive(Debug, Default, Clone, Deserialize)]\n\
                      #[serde(try_from = \"PluginPresetEntries\")]\n\
                      pub struct BabelPresets { pub errors: Vec<String> }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn repro_8075_serde_from_container_not_flagged() {
        let source = "#[derive(Deserialize)]\n\
                      #[serde(from = \"Raw\")]\n\
                      pub struct Cooked { pub v: u8 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn repro_8075_serde_rename_naming_from_still_flagged() {
        // `from` must be a container option, not a value spelled inside another
        // one — `#[serde(rename = "from")]` says nothing about the impl.
        let source = "#[derive(Deserialize)]\n\
                      #[serde(rename = \"from\")]\n\
                      pub struct Cooked { pub v: u8 }";
        assert_eq!(run_on(source).len(), 1);
    }




    #[test]
    fn repro_8323_cfg_gated_field_not_flagged() {
        // ratatui's `Style`: a build with `underline-color` on writes a key a build
        // with it off has no field for. `deny_unknown_fields` is whole-struct, so
        // it would turn that into an `unknown field` error instead of an ignored key.
        let source = "#[derive(Debug, Default, Clone, Copy)]\n\
                      #[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]\n\
                      pub struct Style {\n\
                          pub fg: Option<u8>,\n\
                          #[cfg(feature = \"underline-color\")]\n\
                          pub underline_color: Option<u8>,\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn repro_8323_cfg_target_os_field_not_flagged() {
        // The test is on `cfg`, not on which predicate it carries.
        let source = "#[derive(Deserialize)]\n\
                      pub struct Paths {\n\
                          pub home: String,\n\
                          #[cfg(target_os = \"linux\")]\n\
                          pub xdg: String,\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn repro_8323_cfg_attr_serde_helper_on_field_still_flagged() {
        // A serde helper applied through `cfg_attr` is not a conditional field —
        // the field exists in every build, so the field set is still fixed.
        let source = "#[derive(Deserialize)]\n\
                      pub struct Style {\n\
                          #[cfg_attr(feature = \"serde\", serde(skip_serializing_if = \"Option::is_none\"))]\n\
                          pub fg: Option<u8>,\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn repro_8323_all_public_plain_data_struct_still_flagged() {
        // ratatui's `Rect` — the control the exemptions must not swallow.
        let source = "#[derive(Debug, Default, Clone, Copy)]\n\
                      #[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]\n\
                      pub struct Rect { pub x: u16, pub y: u16, pub width: u16, pub height: u16 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn repro_8323_opaque_round_trip_struct_not_flagged() {
        // ratatui's `ListState`: consumers hold it but cannot name `offset` or
        // `selected`, and the only writer of its serialized form is the library's
        // own `Serialize`. There is no author who could mistype a key.
        let source = "#[derive(Debug, Default, Clone, Copy)]\n\
                      #[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]\n\
                      pub struct ListState {\n\
                          pub(crate) offset: usize,\n\
                          pub(crate) selected: Option<usize>,\n\
                      }";
        assert!(run_in_lib_crate(source).is_empty());
    }

    #[test]
    fn repro_8323_opaque_round_trip_struct_with_bare_private_fields_not_flagged() {
        // ratatui's `ScrollbarState` writes no visibility modifier at all; the
        // fields are just as unreachable as `pub(crate)` ones.
        let source = "#[derive(Debug, Default, Clone, Copy)]\n\
                      #[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]\n\
                      pub struct ScrollbarState {\n\
                          content_length: usize,\n\
                          position: usize,\n\
                      }";
        assert!(run_in_lib_crate(source).is_empty());
    }

    #[test]
    fn repro_8323_publishing_one_field_keeps_the_struct_flagged() {
        // One writable key is enough: a consumer can name `offset` and mistype it.
        let source = "#[derive(Serialize, Deserialize)]\n\
                      pub struct ListState {\n\
                          pub offset: usize,\n\
                          pub(crate) selected: Option<usize>,\n\
                          pub(crate) scroll: usize,\n\
                      }";
        assert_eq!(run_in_lib_crate(source).len(), 1);
    }

    #[test]
    fn repro_8323_deserialize_only_opaque_shape_still_flagged() {
        // No `Serialize`: the crate never writes this document, so somebody
        // outside it does — rust-analyzer's `SnippetDefRepr` is this shape and
        // must stay flagged.
        let source = "#[derive(Deserialize, Default)]\n\
                      #[serde(default)]\n\
                      pub struct SnippetDefRepr {\n\
                          prefix: Vec<String>,\n\
                          body: Vec<String>,\n\
                      }";
        assert_eq!(run_in_lib_crate(source).len(), 1);
    }

    #[test]
    fn repro_8323_opaque_shape_in_a_binary_only_crate_still_flagged() {
        // Nothing links against a binary-only package, so the hidden fields make
        // no encapsulation claim — and a CLI's config type, private by default,
        // is exactly the hand-written document the rule exists for.
        let source = "#[derive(Serialize, Deserialize)]\n\
                      pub struct Config { rate: u32 }";
        assert_eq!(
            crate::rules::test_helpers::run_rule_in_crate(
                &Check,
                crate::rules::test_helpers::BINARY_ONLY_CARGO_TOML,
                source,
            )
            .len(),
            1
        );
    }

    #[test]
    fn repro_8323_pub_struct_confined_to_a_private_module_still_flagged() {
        // helix's `clipboard::external::Command`: `pub` inside a private `mod`,
        // never re-exported, and read straight from the user's `config.toml`.
        let source = "mod external {\n\
                          #[derive(Debug, Clone, Serialize, Deserialize)]\n\
                          pub struct Command {\n\
                              command: String,\n\
                              args: Vec<String>,\n\
                          }\n\
                      }";
        assert_eq!(run_in_lib_crate(source).len(), 1);
    }


}
