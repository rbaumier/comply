//! rust-no-lossy-as-cast backend.
//!
//! Walks `type_cast_expression` nodes (the `expr as Type` syntax)
//! and flags casts where the destination type is in our "narrowing
//! or precision-losing" set:
//!
//! - integer narrowing (`u32 as u8`, `i64 as i32`, etc.)
//! - float to integer (`f64 as u32`)
//! - integer to float when precison can be lost (`u32 as f32`, etc.)
//!
//! Widening casts with the same signedness (e.g. `u8 as u32`) are
//! silenced when the source type is locally visible.  When the source
//! type is not locally annotated (e.g. a method return or a custom
//! type alias), the cast is flagged conservatively.  Use
//! `// comply-ignore: rust-no-lossy-as-cast — <justification>` to
//! suppress known-safe casts in that situation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::cast_operand_is_collection_size;

const KINDS: &[&str] = &["type_cast_expression"];

const NARROWING_TARGETS: &[&str] = &["u8", "u16", "u32", "i8", "i16", "i32", "f32"];

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
        let source_bytes = ctx.source.as_bytes();
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };
        let Ok(target) = type_node.utf8_text(source_bytes) else {
            return;
        };
        let target = target.trim();
        if !NARROWING_TARGETS.contains(&target) {
            return;
        }
        let Some(target_type) = numeric_type(target) else {
            return;
        };
        if source_is_char(node, source_bytes) && char_fits(target_type) {
            return;
        }
        if cast_operand_is_collection_size(node, source_bytes) {
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
            rule_id: "rust-no-lossy-as-cast".into(),
            message: format!(
                "`as {target}` truncates / loses precision silently \
                 on overflow. Use `try_into()` (returns Result) for \
                 fallible narrowing, or `From::from(x)` if the cast \
                 is provably total."
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

/// A `char` is a Unicode scalar value in `0..=0x10FFFF` (21 bits), so a cast
/// to any signed/unsigned integer of at least 21 bits is lossless. Floats are
/// excluded — the rule never claims a float target is safe here.
fn char_fits(target: NumericType) -> bool {
    target.kind != NumericKind::Float && target.bits >= 21
}

/// True when the cast operand is a `char`: a `char_literal` (`'A' as i32`) or
/// an identifier whose local binding is annotated `char` (`c as i32`).
fn source_is_char(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    match value.kind() {
        "char_literal" => true,
        "identifier" => value
            .utf8_text(source)
            .ok()
            .and_then(|name| find_identifier_type(node, name, source))
            .is_some_and(|type_text| type_text == "char"),
        _ => false,
    }
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
    fn flags_u32_to_u8() {
        assert_eq!(run_on("fn f(x: u32) -> u8 { x as u8 }").len(), 1);
    }

    #[test]
    fn flags_f64_to_u32() {
        assert_eq!(run_on("fn f(x: f64) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn allows_widening_to_u64() {
        assert!(run_on("fn f(x: u32) -> u64 { x as u64 }").is_empty());
    }

    #[test]
    fn allows_widening_to_i64() {
        assert!(run_on("fn f(x: i32) -> i64 { x as i64 }").is_empty());
    }

    #[test]
    fn allows_widening_u8_to_u32() {
        assert!(run_on("fn f(x: u8) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn allows_widening_u16_to_u32() {
        assert!(run_on("fn f(x: u16) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn allows_widening_i8_to_i32() {
        assert!(run_on("fn f(x: i8) -> i32 { x as i32 }").is_empty());
    }

    #[test]
    fn allows_widening_i16_to_i32() {
        assert!(run_on("fn f(x: i16) -> i32 { x as i32 }").is_empty());
    }

    #[test]
    fn flags_unknown_source_type_conservatively() {
        assert_eq!(run_on("fn f(x: MyInt) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn allows_char_param_to_i32() {
        // Issue #1430: `char` is a Unicode scalar (0..=0x10FFFF), always fits i32.
        assert!(run_on("fn f(c: char) -> i32 { c as i32 - 64 }").is_empty());
    }

    #[test]
    fn allows_char_param_to_u32() {
        assert!(run_on("fn f(c: char) -> u32 { c as u32 }").is_empty());
    }

    #[test]
    fn allows_char_literal_to_i32() {
        assert!(run_on("fn f() -> i32 { 'A' as i32 }").is_empty());
    }

    #[test]
    fn flags_char_to_u8() {
        // `char as u8` truncates (only the low byte survives).
        assert_eq!(run_on("fn f(c: char) -> u8 { c as u8 }").len(), 1);
    }

    #[test]
    fn flags_char_to_u16() {
        assert_eq!(run_on("fn f(c: char) -> u16 { c as u16 }").len(), 1);
    }

    #[test]
    fn flags_char_literal_to_i8() {
        assert_eq!(run_on("fn f() -> i8 { 'A' as i8 }").len(), 1);
    }

    #[test]
    fn repro_1309_len_as_u32_not_flagged() {
        // A collection's `.len()` cannot exceed `isize::MAX` elements; forcing
        // `try_into` there creates a semantically-impossible error path.
        let src = "fn f(d: D) -> u32 { d.hunks.len() as u32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_1309_self_field_len_as_u32_not_flagged() {
        let src = "fn f(&self) -> u32 { self.diff.hunks.len() as u32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_1309_count_as_u16_not_flagged() {
        assert!(run_on("fn f(v: V) -> u16 { v.iter().count() as u16 }").is_empty());
    }

    #[test]
    fn repro_1309_capacity_as_u32_not_flagged() {
        assert!(run_on("fn f(v: V) -> u32 { v.capacity() as u32 }").is_empty());
    }

    #[test]
    fn repro_1309_unbounded_method_call_still_flagged() {
        // `.parse_count()` is not a collection-size method — keep flagging:
        // the exemption must not blanket-allow every method-call operand.
        assert_eq!(run_on("fn f(v: V) -> u8 { v.parse_count() as u8 }").len(), 1);
    }
}
