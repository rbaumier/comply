//! rust-serde-untagged-without-explicit-default backend.
//!
//! On every `enum_item`, check the preceding `attribute_item` siblings for
//! `#[serde(untagged)]`. If found, walk each variant: for any field whose
//! type is `Option<T>`, ensure the field has its own `#[serde(default)]`
//! attribute. Flag the field otherwise.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["enum_item"];

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
        let source = ctx.source.as_bytes();
        if !has_serde_untagged(node, source) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let mut variant_cursor = body.walk();
        for variant in body.named_children(&mut variant_cursor) {
            if variant.kind() != "enum_variant" {
                continue;
            }
            check_variant(variant, source, ctx, diagnostics);
        }
    }
}

fn check_variant(
    variant: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Variant body is either `field_declaration_list` (struct-style) or
    // `ordered_field_declaration_list` (tuple-style). Walk both.
    let mut cursor = variant.walk();
    for child in variant.named_children(&mut cursor) {
        match child.kind() {
            "field_declaration_list" => check_field_decls(child, source, ctx, diagnostics),
            "ordered_field_declaration_list" => {
                check_ordered_fields(child, source, ctx, diagnostics)
            }
            _ => {}
        }
    }
}

fn check_field_decls(
    list: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = list.walk();
    for field in list.named_children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        let Some(ty) = field.child_by_field_name("type") else {
            continue;
        };
        if !is_option_type(ty, source) {
            continue;
        }
        if has_serde_default(field, source) {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &field,
            "rust-serde-untagged-without-explicit-default",
            "`Option<T>` field in a `#[serde(untagged)]` variant must \
             carry `#[serde(default)]` to make matching deterministic."
                .into(),
            Severity::Warning,
        ));
    }
}

fn check_ordered_fields(
    list: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Tuple variants: `Variant(Option<T>)`. Each named child is a type.
    let mut cursor = list.walk();
    for ty in list.named_children(&mut cursor) {
        if !is_option_type(ty, source) {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &ty,
            "rust-serde-untagged-without-explicit-default",
            "`Option<T>` field in a `#[serde(untagged)]` tuple variant \
             must carry `#[serde(default)]` to make matching deterministic."
                .into(),
            Severity::Warning,
        ));
    }
}

fn is_option_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "generic_type" {
        return false;
    }
    let Some(ty) = node.child_by_field_name("type") else {
        return false;
    };
    ty.utf8_text(source).map(|t| t == "Option").unwrap_or(false)
}

fn has_serde_untagged(item: tree_sitter::Node, source: &[u8]) -> bool {
    each_attribute(item, source, |text| text.contains("untagged"))
}

fn has_serde_default(item: tree_sitter::Node, source: &[u8]) -> bool {
    each_attribute(item, source, |text| text.contains("default"))
}

/// Iterate the `attribute_item` nodes preceding `item` and call `pred` on
/// each text. Returns true on first match. Stops at the first non-attribute
/// sibling, mirroring `has_test_attribute` from rust_helpers.
fn each_attribute(item: tree_sitter::Node, source: &[u8], pred: impl Fn(&str) -> bool) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("serde")
            && pred(text)
        {
            return true;
        }
        sibling = s.prev_named_sibling();
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
    fn flags_untagged_option_field_without_default() {
        let src = r#"
#[derive(Deserialize)]
#[serde(untagged)]
enum E {
    A { x: Option<u32> },
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_untagged_option_with_default() {
        let src = r#"
#[derive(Deserialize)]
#[serde(untagged)]
enum E {
    A {
        #[serde(default)]
        x: Option<u32>,
    },
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_option_field_in_untagged() {
        let src = r#"
#[derive(Deserialize)]
#[serde(untagged)]
enum E {
    A { x: u32 },
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_option_field_in_tagged_enum() {
        let src = r#"
#[derive(Deserialize)]
enum E {
    A { x: Option<u32> },
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_untagged_tuple_variant_with_option() {
        let src = r#"
#[derive(Deserialize)]
#[serde(untagged)]
enum E {
    A(Option<u32>),
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
