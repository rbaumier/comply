//! rust-error-variant-stringly backend.
//!
//! Walks every `enum_item` that is an error type and reports one diagnostic per
//! information-erasing variant. An enum counts as an error type when it derives
//! `Error` (`#[derive(thiserror::Error)]` and any other spelling whose last
//! `::` segment is `Error`), when its name ends in `Error`, or when the file
//! writes `impl std::error::Error for <name>`.
//!
//! Two variant shapes are reported:
//!
//! - a catch-all name — `Other`, `Unknown`, `Internal`, `Generic`, `Custom`,
//!   `Misc` — carrying any payload at all: every unrelated failure lands in one
//!   arm the caller cannot discriminate;
//! - any variant whose only payload is a `String`, tuple (`InvalidRange(String)`)
//!   or named (`Parse { message: String }`): the structured detail is formatted
//!   away at construction.
//!
//! A variant matching both is reported once, under the catch-all message.
//!
//! Exempt shapes:
//!
//! - a catch-all that merely forwards another error — a field carrying `#[from]`
//!   or `#[source]`, a field literally named `source` (thiserror's implicit
//!   source), or a payload typed `anyhow::Error` / `Box<dyn Error + …>`. That
//!   variant keeps the original error whole, which is the opposite of erasing it;
//! - a `#[error(transparent)]` variant, which by contract forwards its inner
//!   error's `Display` and adds nothing of its own;
//! - a variant with two or more fields where the `String` is one detail among
//!   typed ones (`NotFound { kind: Kind, name: String }`);
//! - a unit catch-all (`Other`) — there is no payload to structure;
//! - every enum that is not an error type, so a `Message(String)` in an
//!   `AppEvent` / `Command` enum is out of scope;
//! - test code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    any_outer_attribute, collect_top_level_derives, file_impls_trait_for_type, has_test_attribute,
    is_in_test_context,
};

/// Variant names that promise nothing about the failure they carry. Any payload
/// under one of these is a funnel: the caller matches the arm and is back to
/// reading a message.
const CATCH_ALL_VARIANT_NAMES: &[&str] =
    &["Other", "Unknown", "Internal", "Generic", "Custom", "Misc"];

/// Field attributes that mark the payload as the underlying error, forwarded
/// whole. thiserror generates the `Error::source` wiring from them.
const SOURCE_FIELD_ATTRIBUTES: &[&str] = &["from", "source"];

crate::ast_check! { on ["enum_item"] prefilter = ["enum"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    // `is_in_test_context` reads the ANCESTORS' attributes, so a `#[cfg(test)]`
    // written on this very enum needs its own check.
    if is_in_test_context(node, source) || has_test_attribute(node, source) { return; }

    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let Ok(enum_name) = name_node.utf8_text(source) else { return; };
    if !is_error_enum(node, source, enum_name) { return; }

    let Some(body) = node.child_by_field_name("body") else { return; };
    let mut cursor = body.walk();
    for variant in body.children(&mut cursor) {
        if variant.kind() != "enum_variant" {
            continue;
        }
        let Some(message) = variant_defect(variant, source, enum_name) else {
            continue;
        };
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &variant,
            super::META.id,
            message,
            Severity::Error,
        ));
    }
}

/// The diagnostic message for a variant that erases information, or `None` when
/// the variant is fine. A variant that is both a catch-all and `String`-payloaded
/// yields the catch-all message, so it is reported once.
fn variant_defect(
    variant: tree_sitter::Node,
    source: &[u8],
    enum_name: &str,
) -> Option<String> {
    // `#[error(transparent)]` forwards the inner error's `Display` verbatim; the
    // variant deliberately adds nothing of its own.
    if any_outer_attribute(variant, source, |text| text.contains("transparent")) {
        return None;
    }
    let variant_name = variant.child_by_field_name("name")?.utf8_text(source).ok()?;
    // A unit variant has no `body` field: there is no payload to structure, so
    // neither shape applies.
    let body = variant.child_by_field_name("body")?;

    if CATCH_ALL_VARIANT_NAMES.contains(&variant_name) && !payload_forwards_source(body, source) {
        return Some(format!(
            "Variant `{enum_name}::{variant_name}` is a catch-all carrying an untyped payload — \
             every failure funnelled through it loses its shape and no caller can act on the match. \
             Give it typed fields (`SchemaMismatch {{ expected: Type, got: Type }}`) or forward the \
             real error with `#[from] SourceError`."
        ));
    }
    if payload_is_only_string(body, source) {
        return Some(format!(
            "Variant `{enum_name}::{variant_name}` carries only a `String` — the structured detail is \
             formatted away at construction and the caller gets a sentence it cannot match on. \
             Replace the `String` with typed fields (`SchemaMismatch {{ expected: Type, got: Type }}`) \
             or forward the real error with `#[from] SourceError`."
        ));
    }
    None
}

/// True when the enum is an error type: it derives a trait whose last `::`
/// segment is `Error` (`thiserror::Error`, `derive_more::Error`, a bare
/// `Error`), its name ends in `Error`, or the file implements
/// `std::error::Error` for it.
fn is_error_enum(enum_item: tree_sitter::Node, source: &[u8], enum_name: &str) -> bool {
    if enum_name.ends_with("Error") {
        return true;
    }
    let derives_error = collect_top_level_derives(enum_item, source)
        .iter()
        .any(|entry| entry.rsplit("::").next().unwrap_or(entry).trim() == "Error");
    derives_error || file_impls_trait_for_type(enum_item, source, &["Error"], enum_name)
}

/// True when the variant's payload keeps the underlying error whole rather than
/// flattening it: a `#[from]` / `#[source]` field attribute, a field named
/// `source` (thiserror reads that name as the source without an attribute), or a
/// field typed as an error container that is meant to stay opaque
/// (`anyhow::Error`, `Box<dyn Error + …>`).
fn payload_forwards_source(body: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        let forwards = match child.kind() {
            "attribute_item" => child
                .utf8_text(source)
                .ok()
                .and_then(attribute_path)
                .is_some_and(|path| SOURCE_FIELD_ATTRIBUTES.contains(&path)),
            "field_declaration" => {
                child
                    .child_by_field_name("name")
                    .and_then(|name| name.utf8_text(source).ok())
                    == Some("source")
            }
            _ => false,
        };
        if forwards {
            return true;
        }
    }
    field_types(body)
        .into_iter()
        .filter_map(|ty| ty.utf8_text(source).ok())
        .any(type_is_opaque_error)
}

/// The path of a single-token attribute — `#[from]` → `"from"`. Returns `None`
/// for anything carrying arguments or a value, none of which is a bare source
/// marker.
fn attribute_path(attribute_text: &str) -> Option<&str> {
    let inner = attribute_text
        .trim()
        .strip_prefix("#[")?
        .strip_suffix(']')?
        .trim();
    inner
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then_some(inner)
}

/// True for payload types that hold an error the variant is not expected to
/// destructure: `anyhow::Error` and any `dyn Error` trait object.
fn type_is_opaque_error(type_text: &str) -> bool {
    type_text.contains("anyhow::Error")
        || (type_text.contains("dyn") && type_text.contains("Error"))
}

/// True when the variant's whole payload is one `String`, whether written as a
/// tuple (`Parse(String)`) or as a single named field (`Parse { message: String }`).
fn payload_is_only_string(body: tree_sitter::Node, source: &[u8]) -> bool {
    let types = field_types(body);
    types.len() == 1
        && types[0]
            .utf8_text(source)
            .is_ok_and(|text| text.trim() == "String")
}

/// Every field type of a variant body, for both payload spellings: the `type`
/// fields of an `ordered_field_declaration_list` (tuple variant) or the types of
/// each `field_declaration` in a `field_declaration_list` (struct variant).
fn field_types<'tree>(body: tree_sitter::Node<'tree>) -> Vec<tree_sitter::Node<'tree>> {
    let mut cursor = body.walk();
    match body.kind() {
        "ordered_field_declaration_list" => {
            body.children_by_field_name("type", &mut cursor).collect()
        }
        "field_declaration_list" => body
            .children(&mut cursor)
            .filter(|child| child.kind() == "field_declaration")
            .filter_map(|field| field.child_by_field_name("type"))
            .collect(),
        _ => Vec::new(),
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_string_only_tuple_variant() {
        let found = run("pub enum ParseError { InvalidRange(String) }");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("InvalidRange"), "{}", found[0].message);
    }

    #[test]
    fn flags_string_only_named_variant() {
        assert_eq!(run("pub enum ParseError { Parse { message: String } }").len(), 1);
    }

    #[test]
    fn flags_catch_all_variant_with_typed_payload() {
        assert_eq!(run("pub enum StoreError { Internal(std::io::Error) }").len(), 1);
    }

    #[test]
    fn flags_catch_all_variant_once_even_with_string_payload() {
        let found = run("pub enum StoreError { Other(String) }");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("catch-all"), "{}", found[0].message);
    }

    #[test]
    fn flags_enum_recognized_by_thiserror_derive() {
        let src = "#[derive(Debug, thiserror::Error)]\npub enum Failure { Parse(String) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_enum_recognized_by_error_impl() {
        let src = "pub enum Failure { Parse(String) }\nimpl std::error::Error for Failure {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_every_offending_variant() {
        let src = "pub enum AppError { Other(u32), Parse(String), NotFound { id: u64 } }";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_non_error_enum() {
        assert!(run("pub enum AppEvent { Message(String) }").is_empty());
    }

    #[test]
    fn allows_command_enum_with_string_payload() {
        assert!(run("pub enum Command { Run(String) }").is_empty());
    }

    #[test]
    fn allows_catch_all_forwarding_with_from() {
        assert!(run("pub enum AppError { Other(#[from] anyhow::Error) }").is_empty());
    }

    #[test]
    fn allows_catch_all_forwarding_with_source_attribute() {
        let src = "pub enum AppError { Internal { #[source] cause: std::io::Error } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_catch_all_with_field_named_source() {
        let src = "pub enum AppError { Internal { source: std::io::Error } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_catch_all_over_boxed_dyn_error() {
        let src = "pub enum AppError { Other(Box<dyn std::error::Error + Send + Sync>) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_transparent_variant() {
        let src = "#[derive(thiserror::Error, Debug)]\npub enum AppError { #[error(transparent)] Wrapped(String) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unit_catch_all_variant() {
        assert!(run("pub enum AppError { Other }").is_empty());
    }

    #[test]
    fn allows_string_beside_a_typed_field() {
        assert!(run("pub enum AppError { NotFound { kind: Kind, name: String } }").is_empty());
    }

    #[test]
    fn allows_typed_variant() {
        let src = "#[derive(thiserror::Error, Debug)]\npub enum AppError { SchemaMismatch { expected: Type, got: Type } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_error_enum_in_test_module() {
        assert!(run("#[cfg(test)]\nmod tests { pub enum AppError { Other(String) } }").is_empty());
    }

    #[test]
    fn allows_error_enum_gated_on_cfg_test() {
        assert!(run("#[cfg(test)]\npub enum AppError { Other(String) }").is_empty());
    }
}
