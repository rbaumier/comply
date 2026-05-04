//! rust-no-as-numeric-cast backend.
//!
//! Walks `type_cast_expression` nodes (the `expr as Type` syntax) and
//! flags casts whose destination type is a numeric primitive and whose
//! source/target pair can silently narrow, wrap, or lose precision.
//! Widening integer casts with the same signedness are allowed when the
//! source type is locally obvious.
//!
//! Tests are exempted — fuzz / numeric scaffolding inside `#[test]`
//! functions or `#[cfg(test)]` modules doesn't need this discipline.
//!
//! Non-numeric targets (pointer, reference, trait object) are ignored.
//! Casts like `*const u8 as usize` are false positives; suppress with
//! `// comply-ignore` on the offending line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["type_cast_expression"];

#[derive(Debug)]
pub struct Check;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumericKind {
    Unsigned,
    Signed,
    Float,
}

#[derive(Clone, Copy)]
struct NumericType {
    kind: NumericKind,
    bits: u16,
}

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
        let source_bytes = ctx.source.as_bytes();
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };
        let Ok(target_raw) = type_node.utf8_text(source_bytes) else {
            return;
        };
        let target = target_raw.trim();
        let Some(target_type) = numeric_type(target) else {
            return;
        };
        if target == "usize" || target == "isize" {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        if is_literal_cast(node, source_bytes) {
            return;
        }
        if let Some(source_type) = source_numeric_type(node, source_bytes)
            && !is_dangerous_cast(source_type, target_type)
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-as-numeric-cast".into(),
            message: format!(
                "`as {target}` masks overflow + precision semantics. Use \
                 `{target}::from(x)` for widening-safe casts or \
                 `{target}::try_from(x)?` for fallible narrowing."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn numeric_type(type_text: &str) -> Option<NumericType> {
    let (kind, bits) = match type_text.trim() {
        "u8" => (NumericKind::Unsigned, 8),
        "u16" => (NumericKind::Unsigned, 16),
        "u32" => (NumericKind::Unsigned, 32),
        "u64" => (NumericKind::Unsigned, 64),
        "u128" => (NumericKind::Unsigned, 128),
        "usize" => (NumericKind::Unsigned, usize::BITS as u16),
        "i8" => (NumericKind::Signed, 8),
        "i16" => (NumericKind::Signed, 16),
        "i32" => (NumericKind::Signed, 32),
        "i64" => (NumericKind::Signed, 64),
        "i128" => (NumericKind::Signed, 128),
        "isize" => (NumericKind::Signed, usize::BITS as u16),
        "f32" => (NumericKind::Float, 32),
        "f64" => (NumericKind::Float, 64),
        _ => return None,
    };
    Some(NumericType { kind, bits })
}

fn is_dangerous_cast(source: NumericType, target: NumericType) -> bool {
    if source.kind == target.kind && source.kind != NumericKind::Float {
        return target.bits < source.bits;
    }
    true
}

fn source_numeric_type(node: tree_sitter::Node, source: &[u8]) -> Option<NumericType> {
    let value = node.child_by_field_name("value")?;
    if value.kind() != "identifier" {
        return None;
    }
    let name = value.utf8_text(source).ok()?;
    let type_text = find_identifier_type(node, name, source)?;
    numeric_type(&type_text)
}

fn find_identifier_type(node: tree_sitter::Node, name: &str, source: &[u8]) -> Option<String> {
    let mut current = Some(node);
    while let Some(n) = current {
        if matches!(
            n.kind(),
            "function_item" | "closure_expression" | "block" | "source_file"
        ) && let Some(found) = find_binding_type_before(n, node.start_byte(), name, source)
        {
            return Some(found);
        }
        current = n.parent();
    }
    None
}

fn find_binding_type_before(
    node: tree_sitter::Node,
    limit: usize,
    name: &str,
    source: &[u8],
) -> Option<String> {
    if node.start_byte() >= limit {
        return None;
    }
    if matches!(node.kind(), "parameter" | "let_declaration")
        && let Some(pattern) = node.child_by_field_name("pattern")
        && pattern_contains_identifier(pattern, name, source)
        && let Some(type_node) = node.child_by_field_name("type")
        && let Ok(type_text) = type_node.utf8_text(source)
    {
        return Some(type_text.trim().to_string());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_binding_type_before(child, limit, name, source) {
            return Some(found);
        }
    }
    None
}

fn pattern_contains_identifier(pattern: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if pattern.kind() == "identifier" {
        return pattern.utf8_text(source).is_ok_and(|text| text == name);
    }

    let mut cursor = pattern.walk();
    pattern
        .children(&mut cursor)
        .any(|child| pattern_contains_identifier(child, name, source))
}

fn is_literal_cast(node: tree_sitter::Node, _source: &[u8]) -> bool {
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    matches!(
        value.kind(),
        "integer_literal" | "float_literal" | "unary_expression"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn allows_widening_u8_to_u64() {
        assert!(run_on("fn f(x: u8) -> u64 { x as u64 }").is_empty());
    }

    #[test]
    fn allows_widening_i32_to_i64() {
        assert!(run_on("fn f(x: i32) -> i64 { x as i64 }").is_empty());
    }

    #[test]
    fn flags_narrowing_u64_to_u8() {
        assert_eq!(run_on("fn f(x: u64) -> u8 { x as u8 }").len(), 1);
    }

    #[test]
    fn flags_float_cast() {
        assert_eq!(run_on("fn f(x: i32) -> f64 { x as f64 }").len(), 1);
    }

    #[test]
    fn flags_signed_to_unsigned() {
        assert_eq!(run_on("fn f(x: i32) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn flags_unknown_source_type_conservatively() {
        assert_eq!(run_on("fn f(x: MyInt) -> u64 { x as u64 }").len(), 1);
    }

    #[test]
    fn allows_as_usize() {
        assert!(run_on("fn f(x: u32) -> usize { x as usize }").is_empty());
    }

    #[test]
    fn allows_as_isize() {
        assert!(run_on("fn f(x: i32) -> isize { x as isize }").is_empty());
    }

    #[test]
    fn allows_literal_cast() {
        assert!(run_on("fn f() { let _ = 42 as u8; }").is_empty());
        assert!(run_on("fn f() { let _ = 1.0 as f32; }").is_empty());
    }

    #[test]
    fn allows_non_numeric_target() {
        assert!(run_on("fn f(x: &str) -> &[u8] { x as &[u8] }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let source = "#[test]\nfn t() { let _ = 1u8 as u64; }";
        assert!(run_on(source).is_empty());
    }
}
