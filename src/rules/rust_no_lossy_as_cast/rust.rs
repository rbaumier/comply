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
use crate::rules::rust_helpers::{
    cast_operand_is_bool, cast_operand_is_collection_size, find_identifier_type,
};

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
        if cast_operand_is_bool(node, source_bytes) {
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

/// True when the cast operand is a `char`: a `char_literal` (`'A' as i32`), an
/// identifier whose local binding is annotated `char` (`c as i32`), or an
/// identifier bound by a `chars()`/`char_indices()` for-loop (`for c in
/// s.chars()`).
fn source_is_char(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    match value.kind() {
        "char_literal" => true,
        "identifier" => value.utf8_text(source).ok().is_some_and(|name| {
            find_identifier_type(node, name, source)
                .is_some_and(|type_text| type_text == "char")
                || binding_is_chars_iter(node, name, source)
        }),
        _ => false,
    }
}

/// True when `name` is the `char` binding of an enclosing `for <pat> in
/// <expr>.chars()` or `for (<idx>, <name>) in <expr>.char_indices()` loop.
///
/// `<str>.chars()` yields `char`, and `<str>.char_indices()` yields `(usize,
/// char)` — so the plain loop binding (or the tuple's second element) is a
/// `char`. The match is on the iterator's method name, not the receiver, since
/// any `&str`/`String` chain ending in those inherent methods yields a `char`.
fn binding_is_chars_iter(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "for_expression"
            && let Some(pattern) = n.child_by_field_name("pattern")
            && for_pattern_binds_char(pattern, name, source)
            && let Some(value) = n.child_by_field_name("value")
            && let Some(method) = chars_iter_method(value, source)
        {
            return (method == "chars" && pattern.kind() == "identifier")
                || (method == "char_indices" && pattern.kind() == "tuple_pattern");
        }
        current = n.parent();
    }
    false
}

/// True when `pattern` is the for-loop binding site for the `char` value of a
/// `chars()`/`char_indices()` iterator: either the plain identifier `name`
/// (`for name in ...chars()`), or the second element of a two-element tuple
/// pattern (`for (_, name) in ...char_indices()`).
fn for_pattern_binds_char(pattern: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    match pattern.kind() {
        "identifier" => pattern.utf8_text(source).is_ok_and(|text| text == name),
        "tuple_pattern" => {
            pattern.named_child_count() == 2
                && pattern.named_child(1).is_some_and(|second| {
                    second.kind() == "identifier"
                        && second.utf8_text(source).is_ok_and(|text| text == name)
                })
        }
        _ => false,
    }
}

/// The method name of a no-argument `<expr>.<method>()` call, or `None` if the
/// node is not such a method call.
fn chars_iter_method<'a>(value: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if value.kind() != "call_expression" {
        return None;
    }
    if value
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return None;
    }
    let function = value.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }
    function
        .child_by_field_name("field")
        .and_then(|field| field.utf8_text(source).ok())
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

    #[test]
    fn repro_3949_is_some_as_u8_not_flagged() {
        // `bool as u8` is total and lossless; `is_some()` yields a bool.
        assert!(run_on("fn f(o: Option<i32>) -> u8 { o.is_some() as u8 }").is_empty());
    }

    #[test]
    fn repro_3949_bool_binding_as_u8_not_flagged() {
        assert!(run_on("fn g(b: bool) -> u8 { b as u8 }").is_empty());
    }

    #[test]
    fn repro_3949_comparison_as_u8_not_flagged() {
        assert!(run_on("fn h() -> u8 { (3 > 2) as u8 }").is_empty());
    }

    #[test]
    fn repro_3949_contains_as_u8_not_flagged() {
        assert!(run_on("fn k(s: &str) -> u8 { s.contains(\"x\") as u8 }").is_empty());
    }

    #[test]
    fn repro_3949_bitwise_not_int_narrowing_still_flagged() {
        // `!x` on a u32 is bitwise NOT (stays u32); narrowing to u8 is lossy.
        assert_eq!(run_on("fn f(x: u32) -> u8 { !x as u8 }").len(), 1);
    }

    #[test]
    fn repro_3847_for_chars_binding_as_u32_not_flagged() {
        // `for c in s.chars()` binds `c: char`; `char as u32` is total.
        assert!(run_on("fn f(s: &str) { for c in s.chars() { let _ = c as u32; } }").is_empty());
    }

    #[test]
    fn repro_3847_for_chars_binding_as_u8_still_flagged() {
        // The binding is `char`, but `char as u8` narrows below 21 bits (lossy).
        assert_eq!(
            run_on("fn f(s: &str) { for c in s.chars() { let _ = c as u8; } }").len(),
            1
        );
    }

    #[test]
    fn repro_3847_for_char_indices_binding_as_u32_not_flagged() {
        // `for (i, c) in s.char_indices()` binds `c: char` (the tuple's 2nd elem).
        assert!(
            run_on("fn f(s: &str) { for (i, c) in s.char_indices() { let _ = c as u32; } }")
                .is_empty()
        );
    }

    #[test]
    fn repro_3847_for_non_chars_iter_binding_still_flagged() {
        // The iterator is not `.chars()`/`.char_indices()`, so the binding type
        // is unknown and a narrowing cast must stay flagged.
        assert_eq!(
            run_on("fn f(v: V) { for x in v.bytes() { let _ = x as u8; } }").len(),
            1
        );
    }

    #[test]
    fn repro_3847_inner_loop_shadows_chars_binding_still_flagged() {
        // The innermost `for c` rebinds `c` to a non-char; the nearest binding
        // wins, so `c as u32` must not borrow the outer `chars()` exemption.
        let src = "fn f(s: &str, v: V) { for c in s.chars() { for c in v.iter() { let _ = c as u32; } } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
