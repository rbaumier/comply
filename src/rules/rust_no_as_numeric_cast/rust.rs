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
//!
//! Casts whose operand's outermost expression is a bitwise op
//! (`>>`, `<<`, `&`, `|`, `^`, parens transparent) are bit manipulation —
//! e.g. `(x >> 8) as u8`, `(x & 0xFF) as u8`. The truncation is intentional,
//! so `try_from` would be wrong; these are left alone.
//!
//! Float-target casts (`as f32` / `as f64`) are only flagged when the
//! source type is statically known to have a matching `From` impl
//! (`f64: From<{i8,i16,i32,u8,u16,u32,f32}>`, `f32: From<{i8,i16,u8,u16}>`).
//! `as` is the only std conversion for wider sources (`u64`, `usize`,
//! `u128`, …) and for operands whose type can't be resolved from the AST
//! (method calls, field accesses, un-annotated bindings), so those are
//! left alone — suggesting `f64::from(x)` there would not compile.

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
        if is_bitwise_operand(node, source_bytes) {
            return;
        }
        let source_type = source_numeric_type(node, source_bytes);
        if target_type.kind == NumericKind::Float {
            // `as f32`/`as f64` is the only std conversion unless the source
            // has a matching `From` impl. Suggesting `f64::from(x)` for a
            // wider or unresolved source would not compile, so only flag
            // when `From` is provably available.
            if !source_type.is_some_and(|src| from_available(src, target_type)) {
                return;
            }
        } else if let Some(src) = source_type
            && !is_dangerous_cast(src, target_type)
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

/// Whether `<target as float>::from(<source>)` compiles.
///
/// `f64: From<T>` for `T ∈ {i8,i16,i32,u8,u16,u32,f32}`;
/// `f32: From<T>` for `T ∈ {i8,i16,u8,u16}`. No `From` exists for wider
/// integers (`u64`, `i64`, `usize`, …) or for the lossy `f64 -> f32`.
fn from_available(source: NumericType, target: NumericType) -> bool {
    match target.bits {
        64 => match source.kind {
            NumericKind::Unsigned | NumericKind::Signed => source.bits <= 32,
            NumericKind::Float => source.bits == 32,
        },
        32 => matches!(source.kind, NumericKind::Unsigned | NumericKind::Signed)
            && source.bits <= 16,
        _ => false,
    }
}

fn is_dangerous_cast(source: NumericType, target: NumericType) -> bool {
    match (source.kind, target.kind) {
        (_, NumericKind::Float) | (NumericKind::Float, _) => true,
        (k, k2) if k == k2 => target.bits < source.bits,
        _ => source.bits >= target.bits,
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

/// Whether the cast operand's outermost expression is a bitwise operation
/// (`>>`, `<<`, `&`, `|`, `^`). Such a cast is bit manipulation — the
/// truncation to the target width is intentional (`(x >> 8) as u8`,
/// `(x & 0xFF) as u8`), so `try_from` would be semantically wrong. Parens
/// around the operand are transparent.
fn is_bitwise_operand(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(mut value) = node.child_by_field_name("value") else {
        return false;
    };
    while value.kind() == "parenthesized_expression" {
        let Some(inner) = value.named_child(0) else {
            return false;
        };
        value = inner;
    }
    if value.kind() != "binary_expression" {
        return false;
    }
    value
        .child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| matches!(op, ">>" | "<<" | "&" | "|" | "^"))
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

    #[test]
    fn test_allows_safe_widening_i8_to_u32() {
        assert!(run_on("fn f(x: i8) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn test_allows_safe_widening_i32_to_u64() {
        assert!(run_on("fn f(x: i32) -> u64 { x as u64 }").is_empty());
    }

    #[test]
    fn test_allows_safe_widening_i16_to_u32() {
        assert!(run_on("fn f(x: i16) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn test_flags_dangerous_narrowing_i32_to_u16() {
        assert_eq!(run_on("fn f(x: i32) -> u16 { x as u16 }").len(), 1);
    }

    #[test]
    fn test_flags_dangerous_narrowing_i64_to_u32() {
        assert_eq!(run_on("fn f(x: i64) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn repro_1253_method_call_as_f64_not_flagged() {
        // `as_millis()` returns u128; `f64::from(u128)` does not compile.
        let src = "fn f(d: Duration) -> f64 { d.as_millis() as f64 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_1253_usize_binding_as_f64_not_flagged() {
        // usize is wider than u32 on 64-bit; `f64::from(usize)` does not compile.
        let src = "fn f(n: usize) -> f64 { n as f64 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_1253_u64_binding_as_f64_not_flagged() {
        // `f64::from(u64)` does not compile.
        assert!(run_on("fn f(x: u64) -> f64 { x as f64 }").is_empty());
    }

    #[test]
    fn repro_1253_field_access_as_f64_not_flagged() {
        // Field access — source type not resolvable from the AST.
        assert!(run_on("fn f(s: S) -> f64 { s.count as f64 }").is_empty());
    }

    #[test]
    fn repro_1253_from_compatible_i32_as_f64_still_flagged() {
        // `f64::from(i32)` compiles — the rule should keep flagging this.
        assert_eq!(run_on("fn f(x: i32) -> f64 { x as f64 }").len(), 1);
    }

    #[test]
    fn flags_from_compatible_u8_as_f32() {
        // `f32::from(u8)` compiles — keep flagging.
        assert_eq!(run_on("fn f(x: u8) -> f32 { x as f32 }").len(), 1);
    }

    #[test]
    fn allows_u32_as_f32_no_from_impl() {
        // `f32: From<u32>` does not exist (lossy) — `as` is correct here.
        assert!(run_on("fn f(x: u32) -> f32 { x as f32 }").is_empty());
    }

    #[test]
    fn repro_1289_shift_narrowing_not_flagged() {
        // `(x >> 8) as u8` — bit extraction, truncation intentional.
        assert!(run_on("fn f(x: u32) -> u8 { (x >> 8) as u8 }").is_empty());
    }

    #[test]
    fn repro_1289_mask_narrowing_not_flagged() {
        // `(x & 0xFF) as u8` — masked low byte, truncation intentional.
        assert!(run_on("fn f(x: u32) -> u8 { (x & 0xFF) as u8 }").is_empty());
    }

    #[test]
    fn repro_1289_or_narrowing_not_flagged() {
        assert!(run_on("fn f(a: u32, b: u32) -> u16 { (a | b) as u16 }").is_empty());
    }

    #[test]
    fn repro_1289_xor_shift_not_flagged() {
        assert!(run_on("fn f(a: u32, b: u32) -> u8 { (a ^ b) as u8 }").is_empty());
    }

    #[test]
    fn repro_1289_plain_narrowing_still_flagged() {
        // No bitwise context — an arbitrary count/length narrowing stays flagged.
        assert_eq!(run_on("fn f(n: u32) -> u8 { n as u8 }").len(), 1);
    }
}
