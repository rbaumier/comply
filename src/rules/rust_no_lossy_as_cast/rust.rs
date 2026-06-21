//! rust-no-lossy-as-cast backend.
//!
//! Walks `type_cast_expression` nodes (the `expr as Type` syntax)
//! and flags casts where the destination type is in our "narrowing
//! or precision-losing" set:
//!
//! - integer narrowing (`u32 as u8`, `i64 as i32`, etc.)
//! - float to integer (`f64 as u32`)
//! - integer to `f32` when the source type is resolvable and exceeds the
//!   mantissa (`u32 as f32`, `i32 as f32`). Small integers that fit exactly
//!   — `{i8,u8,i16,u16} as f32` — are lossless and silenced, and a cast
//!   whose source type can't be resolved from the AST (index/field/method
//!   operands, e.g. `gx[(x, y)][0] as f32`) is left to
//!   `rust-no-as-numeric-cast`, since precision loss is not provable there.
//!
//! Same-width signed/unsigned reinterpretations (`u8 as i8`, `i32 as u32`,
//! …) preserve every bit — only the sign bit's interpretation changes via
//! two's complement — so they are not lossy and are silenced when the
//! source type is locally visible.
//!
//! Widening casts with the same signedness (e.g. `u8 as u32`) are
//! silenced when the source type is locally visible, as is an unsigned
//! source cast to a strictly wider signed target (`u16 as i32`): the
//! extra bit accommodates the sign, so every value is represented
//! exactly.  A dereference operand (`*x as i32` where `x: &u16`) resolves
//! through its referent — the leading borrow is stripped and `u16 as i32`
//! is analysed as the widening cast it is.  When the source type is not
//! locally annotated
//! (e.g. a method return or a custom type alias), the cast is flagged
//! conservatively.  Use
//! `// comply-ignore: rust-no-lossy-as-cast — <justification>` to
//! suppress known-safe casts in that situation.
//!
//! A narrowing cast of an unsigned identifier guarded by an enclosing
//! `if`/`else if` upper bound that proves the value fits the target type —
//! `if val < 256 { val as u8 }` — is exempt: the branch is entered only when
//! the value is in range, so the cast cannot overflow.
//!
//! Likewise, a narrowing cast of an unsigned identifier bounded by a preceding
//! `assert!` / `debug_assert!` in the same block whose condition upper-bounds
//! that identifier to the target's range — `assert!(x <= u8::MAX as u64);
//! let y = x as u8;` — is exempt: the assertion aborts before the cast unless
//! the value fits.
//!
//! Casts that `rust-no-as-numeric-cast` already flags are suppressed here so
//! the pair emits one diagnostic per span; this rule keeps firing only where
//! that one does not — notably int `as f32` (no `f32::From` for those sources).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    cast_operand_is_assert_bounded, cast_operand_is_bitwise, cast_operand_is_bool,
    cast_operand_is_char, cast_operand_is_collection_size, cast_operand_is_enum_discriminant,
    cast_operand_is_range_guarded, find_identifier_type, is_in_enum_discriminant,
};
use crate::rules::rust_no_as_numeric_cast::rust::fires_on_cast;

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
        if cast_operand_is_char(node, source_bytes) && char_fits(target_type) {
            return;
        }
        if cast_operand_is_collection_size(node, source_bytes) {
            return;
        }
        if cast_operand_is_bool(node, source_bytes) {
            return;
        }
        if cast_operand_is_enum_discriminant(node, source_bytes) {
            return;
        }
        if cast_operand_is_range_guarded(node, source_bytes) {
            return;
        }
        if cast_operand_is_assert_bounded(node, source_bytes) {
            return;
        }
        if cast_operand_is_bitwise(node, source_bytes) {
            return;
        }
        let source_type = source_numeric_type(node, source_bytes);
        if let Some(source_type) = source_type
            && !is_dangerous_cast(source_type, target_type)
        {
            return;
        }
        // A float target only loses precision when the source is wide enough to
        // overflow the mantissa. That can only be proven from a resolvable
        // source type; for an unresolved operand (index/field/method, e.g.
        // `gx[(x, y)][0] as f32`) the loss is not provable, so defer to
        // `rust-no-as-numeric-cast`, which flags float casts only when a
        // matching `From` impl makes `T::from(x)` a compiling suggestion.
        if target_type.kind == NumericKind::Float && source_type.is_none() {
            return;
        }
        if is_in_enum_discriminant(node) {
            return;
        }
        // De-duplicate: when `rust-no-as-numeric-cast` owns this cast, let it
        // be the single diagnostic for the span. This rule keeps firing only
        // where that one does not (e.g. int/float `as f32`).
        if fires_on_cast(node, source_bytes) {
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

/// `f32` has a 24-bit mantissa, so integers up to 24 bits wide are exactly
/// representable.
const F32_MANTISSA_BITS: u16 = 24;

fn is_dangerous_cast(source: NumericType, target: NumericType) -> bool {
    if source.kind == target.kind && source.kind != NumericKind::Float {
        return target.bits < source.bits;
    }
    // Same-width signed/unsigned reinterpretation (`u8 as i8`, `i32 as u32`, …)
    // preserves every bit — it only reinterprets the sign bit via two's
    // complement, so it is not a lossy cast. `try_into()` would be wrong here:
    // `200_u8.try_into::<i8>()` errors, but the intended result is the bit
    // pattern `-56_i8`.
    if matches!(source.kind, NumericKind::Unsigned | NumericKind::Signed)
        && matches!(target.kind, NumericKind::Unsigned | NumericKind::Signed)
        && source.bits == target.bits
    {
        return false;
    }
    // Unsigned -> signed of strictly greater width (`u16 as i32`, `u8 as i16`)
    // is a widening cast: the target's extra bit accommodates the sign, so
    // every source value is represented exactly. (`u16 as i16` is excluded —
    // same width, handled as a reinterpretation above.)
    if source.kind == NumericKind::Unsigned
        && target.kind == NumericKind::Signed
        && target.bits > source.bits
    {
        return false;
    }
    if target.kind == NumericKind::Float
        && matches!(source.kind, NumericKind::Unsigned | NumericKind::Signed)
    {
        // Integer -> `f32` is lossless when the source fits the mantissa:
        // `{i8,u8,i16,u16} as f32`. `i32`/`u32` (32 bits) overflow it.
        return source.bits > F32_MANTISSA_BITS;
    }
    true
}

/// A `char` is a Unicode scalar value in `0..=0x10FFFF` (21 bits), so a cast
/// to any signed/unsigned integer of at least 21 bits is lossless. Floats are
/// excluded — the rule never claims a float target is safe here.
fn char_fits(target: NumericType) -> bool {
    target.kind != NumericKind::Float && target.bits >= 21
}

fn source_numeric_type(node: tree_sitter::Node, source: &[u8]) -> Option<NumericType> {
    let value = node.child_by_field_name("value")?;
    let ident = deref_identifier(value, source).unwrap_or(value);
    if ident.kind() != "identifier" {
        return None;
    }
    let name = ident.utf8_text(source).ok()?;
    let type_text = find_identifier_type(node, name, source)?;
    // For a dereference operand (`*x`), the binding type is a reference
    // (`&u16` / `&mut u16`); the cast acts on the referent, so strip the
    // leading borrow and analyse the underlying numeric type.
    let type_text = referent_type(&type_text);
    numeric_type(type_text)
}

/// Strip a single leading `&` / `&mut` borrow from a type's source text so a
/// dereferenced operand resolves to its referent (`&u16` → `u16`). A non-
/// reference type is returned unchanged.
fn referent_type(type_text: &str) -> &str {
    match type_text.trim_start().strip_prefix('&') {
        Some(rest) => rest.trim_start().strip_prefix("mut ").unwrap_or(rest).trim_start(),
        None => type_text,
    }
}

/// If `value` is a unary dereference of an identifier (`*x`), return the inner
/// identifier node; otherwise `None`. Peels a single parenthesized wrapper so
/// `(*x) as i32` is covered too.
fn deref_identifier<'a>(value: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
    if value.kind() == "parenthesized_expression" {
        return value.named_child(0).and_then(|inner| deref_identifier(inner, source));
    }
    if value.kind() != "unary_expression" {
        return None;
    }
    let is_deref = value
        .child(0)
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|op| op == "*");
    if !is_deref {
        return None;
    }
    value
        .named_child(0)
        .filter(|operand| operand.kind() == "identifier")
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
    fn u32_to_u8_owned_by_numeric_cast() {
        // Narrowing `u32 as u8` — `rust-no-as-numeric-cast` owns the span.
        assert!(run_on("fn f(x: u32) -> u8 { x as u8 }").is_empty());
    }

    #[test]
    fn f64_to_u32_owned_by_numeric_cast() {
        assert!(run_on("fn f(x: f64) -> u32 { x as u32 }").is_empty());
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
    fn allows_widening_u16_to_i32() {
        // Unsigned -> strictly wider signed: every u16 fits in i32 (the extra
        // bit covers the sign), so the cast is lossless.
        assert!(run_on("fn f(x: u16) -> i32 { x as i32 }").is_empty());
    }

    #[test]
    fn allows_widening_u8_to_i16() {
        assert!(run_on("fn f(x: u8) -> i16 { x as i16 }").is_empty());
    }

    #[test]
    fn repro_4807_u8_as_i8_not_flagged() {
        // Issue #4807: `u8 as i8` is a same-width two's-complement bit
        // reinterpretation (gluon's `(b as i8) >= -0x40` parser bit magic) —
        // no bits are lost, only the sign interpretation changes.
        assert!(run_on("fn is_boundary(b: u8) -> bool { (b as i8) >= -0x40 }").is_empty());
    }

    #[test]
    fn repro_4807_i8_as_u8_not_flagged() {
        assert!(run_on("fn f(b: i8) -> u8 { b as u8 }").is_empty());
    }

    #[test]
    fn repro_4807_u16_as_i16_not_flagged() {
        assert!(run_on("fn f(b: u16) -> i16 { b as i16 }").is_empty());
    }

    #[test]
    fn repro_4807_u32_as_i32_not_flagged() {
        assert!(run_on("fn f(b: u32) -> i32 { b as i32 }").is_empty());
    }

    #[test]
    fn repro_4807_i64_as_u64_not_flagged() {
        assert!(run_on("fn f(b: i64) -> u64 { b as u64 }").is_empty());
    }

    #[test]
    fn repro_4807_usize_as_isize_not_flagged() {
        assert!(run_on("fn f(b: usize) -> isize { b as isize }").is_empty());
    }

    #[test]
    fn cross_signed_narrowing_u32_as_i8_owned_by_numeric_cast() {
        // Different widths: `u32 as i8` discards 24 bits — genuinely lossy and
        // not exempted by the same-width carve-out. `rust-no-as-numeric-cast`
        // owns the span, so this rule suppresses its diagnostic.
        assert!(run_on("fn f(x: u32) -> i8 { x as i8 }").is_empty());
    }

    #[test]
    fn unknown_source_type_owned_by_numeric_cast() {
        // Unresolved source narrowing to u32 — `rust-no-as-numeric-cast` owns it.
        assert!(run_on("fn f(x: MyInt) -> u32 { x as u32 }").is_empty());
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
    fn char_to_u8_owned_by_numeric_cast() {
        // `char as u8` truncates — `rust-no-as-numeric-cast` owns the span.
        assert!(run_on("fn f(c: char) -> u8 { c as u8 }").is_empty());
    }

    #[test]
    fn char_to_u16_owned_by_numeric_cast() {
        assert!(run_on("fn f(c: char) -> u16 { c as u16 }").is_empty());
    }

    #[test]
    fn char_literal_to_i8_owned_by_numeric_cast() {
        assert!(run_on("fn f() -> i8 { 'A' as i8 }").is_empty());
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
    fn repro_1309_unbounded_method_call_owned_by_numeric_cast() {
        // `.parse_count()` is not a collection-size method, so this narrowing
        // is a real finding — but `rust-no-as-numeric-cast` owns the span.
        assert!(run_on("fn f(v: V) -> u8 { v.parse_count() as u8 }").is_empty());
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
    fn repro_3847_for_chars_binding_as_u8_owned_by_numeric_cast() {
        // The binding is `char`, `char as u8` narrows below 21 bits (lossy) —
        // but `rust-no-as-numeric-cast` owns the span.
        assert!(
            run_on("fn f(s: &str) { for c in s.chars() { let _ = c as u8; } }").is_empty()
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
    fn repro_3847_for_non_chars_iter_binding_owned_by_numeric_cast() {
        // The iterator is not `.chars()`/`.char_indices()`, so the binding type
        // is unknown — a real narrowing, but `rust-no-as-numeric-cast` owns it.
        assert!(
            run_on("fn f(v: V) { for x in v.bytes() { let _ = x as u8; } }").is_empty()
        );
    }

    #[test]
    fn repro_3847_inner_loop_shadows_chars_binding_owned_by_numeric_cast() {
        // The innermost `for c` rebinds `c` to a non-char; the nearest binding
        // wins, so `c as u32` is a real narrowing — `rust-no-as-numeric-cast`
        // owns the span.
        let src = "fn f(s: &str, v: V) { for c in s.chars() { for c in v.iter() { let _ = c as u32; } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_3859_enum_discriminant_cast_not_flagged() {
        // A discriminant must be a const expression; `as` is the only
        // conversion that compiles there (`From`/`TryFrom` are unavailable).
        assert!(run_on("#[repr(i8)] enum E { Str = b's' as i8 }").is_empty());
    }

    #[test]
    fn repro_3859_full_conversion_flag_shape_not_flagged() {
        let src = "#[repr(i8)] enum ConversionFlag { None = -1, Str = b's' as i8, Ascii = b'a' as i8, Repr = b'r' as i8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_3859_cast_in_impl_method_owned_by_numeric_cast() {
        // A cast inside an `impl Enum` method is a runtime body, not a
        // discriminant — a real narrowing, but `rust-no-as-numeric-cast`
        // owns the span.
        let src = "enum E { A } impl E { fn f(&self, x: u32) -> i8 { x as i8 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_3832_self_as_u8_in_impl_fieldless_enum_not_flagged() {
        // `self as u8` reads a fieldless enum's discriminant; `as` is the only
        // conversion that compiles (no `From`/`TryFrom<ArgSettings> for u8`).
        let src = "enum ArgSettings { Required, Multiple, Hidden } \
                   impl ArgSettings { fn bit(self) -> u32 { 1 << (self as u8) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_3832_scoped_variant_of_fieldless_enum_not_flagged() {
        let src = "enum E { A, B, C } fn f() -> u8 { E::A as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_3832_self_as_u8_in_impl_data_enum_owned_by_numeric_cast() {
        // A data-carrying enum has no discriminant `as`-cast semantics, so the
        // exemption does not apply — a real narrowing, but
        // `rust-no-as-numeric-cast` owns the span.
        let src = "enum E { A(u32), B } impl E { fn bit(self) -> u8 { self as u8 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_1254_method_call_as_u8_suppressed_numeric_cast_owns_it() {
        // A method-call narrowing both rules used to flag (`.value()` is not a
        // size accessor, so the collection-size exemption does not apply) —
        // `rust-no-as-numeric-cast` owns it, so this rule suppresses.
        assert!(run_on("fn f(m: M) -> u8 { let n = m.value() as u8; n }").is_empty());
    }

    #[test]
    fn repro_1254_axum_index_as_u32_suppressed() {
        // axum `syn::Index { index: index as u32 }` where `index: usize`:
        // `rust-no-as-numeric-cast` fires (unsigned narrowing), so suppress here.
        assert!(run_on("fn f(index: usize) -> u32 { index as u32 }").is_empty());
    }

    #[test]
    fn repro_1254_int_as_f32_still_flagged_numeric_cast_skips_it() {
        // `y as f32` where `y: u32`: `rust-no-as-numeric-cast` skips int->f32
        // (no `f32::From<u32>` to suggest), so this rule still owns it.
        assert_eq!(run_on("fn f(y: u32) -> f32 { let x = y as f32; x }").len(), 1);
    }

    #[test]
    fn repro_4677_i16_as_f32_not_flagged() {
        // Issue #4677: `i16 as f32` is always lossless — i16's 16-bit range
        // fits exactly in f32's 24-bit mantissa.
        assert!(run_on("fn f(g: i16) -> f32 { g as f32 }").is_empty());
    }

    #[test]
    fn repro_4677_u16_as_f32_not_flagged() {
        assert!(run_on("fn f(g: u16) -> f32 { g as f32 }").is_empty());
    }

    #[test]
    fn repro_4677_i8_as_f32_not_flagged() {
        assert!(run_on("fn f(g: i8) -> f32 { g as f32 }").is_empty());
    }

    #[test]
    fn repro_4677_u8_as_f32_not_flagged() {
        assert!(run_on("fn f(g: u8) -> f32 { g as f32 }").is_empty());
    }

    #[test]
    fn repro_4677_i32_as_f32_still_flagged() {
        // i32 (32 bits) exceeds f32's 24-bit mantissa — genuinely lossy.
        assert_eq!(run_on("fn f(g: i32) -> f32 { g as f32 }").len(), 1);
    }

    #[test]
    fn repro_4677_index_operand_as_f32_not_flagged() {
        // The issue's exact shape: `gx[(x, y)][0] as f32`. The operand is an
        // index expression — source type unresolvable from the AST, so the
        // loss is not provable and the rule defers to `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(gx: G) -> f32 { gx[(0, 0)][0] as f32 }").is_empty());
    }

    #[test]
    fn repro_4677_field_operand_as_f32_not_flagged() {
        // A field access is equally unresolvable.
        assert!(run_on("fn f(s: S) -> f32 { s.gradient as f32 }").is_empty());
    }

    #[test]
    fn repro_4677_method_operand_as_f32_not_flagged() {
        assert!(run_on("fn f(s: S) -> f32 { s.value() as f32 }").is_empty());
    }

    #[test]
    fn repro_4922_range_guarded_narrowing_not_flagged() {
        // The msgpack encoder pattern (rmp/src/encode/uint.rs): each `as`
        // narrowing is guarded by an `if`/`else if` upper bound proving the
        // value fits the target type. The final `else` widens to u64 (not
        // flagged).
        let src = "fn w(val: u64) -> u8 { \
                   if val < 256 { val as u8 } \
                   else if val < 65536 { val as u16 } \
                   else if val < 4294967296 { val as u32 } \
                   else { val } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_4922_inclusive_guard_not_flagged() {
        assert!(
            run_on("fn w(val: u64) -> u8 { if val <= 255 { val as u8 } else { 0 } }").is_empty()
        );
    }

    #[test]
    fn repro_4922_unguarded_narrowing_owned_by_numeric_cast() {
        // No range guard — a real narrowing, but `rust-no-as-numeric-cast`
        // owns the span, so this rule suppresses.
        assert!(run_on("fn f(n: u64) -> u8 { n as u8 }").is_empty());
    }

    #[test]
    fn repro_4922_loose_guard_owned_by_numeric_cast() {
        // The bound exceeds u8's range; the narrowing stays a finding but
        // `rust-no-as-numeric-cast` owns the span.
        let src = "fn w(val: u64) -> u8 { if val < 1000 { val as u8 } else { 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5117_deref_ref_u16_as_i32_not_flagged() {
        // Issue #5117 (chumsky pratt.rs): `*x as i32` where `x: &u16`. The
        // deref yields u16, and `u16 as i32` is a widening cast — every u16
        // fits in i32. The parameter `x` is `&u16`, so the rule must see
        // through the deref and strip the borrow.
        let src = "fn p(x: &u16) -> i32 { *x as i32 * 2 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5117_deref_let_ref_u16_as_i32_not_flagged() {
        let src = "fn p(r: &u16) -> i32 { let x: &u16 = r; *x as i32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5117_deref_mut_ref_u16_as_i32_not_flagged() {
        let src = "fn p(x: &mut u16) -> i32 { *x as i32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5117_deref_i16_as_i32_not_flagged() {
        // Same-signedness widening through a deref.
        let src = "fn p(x: &i16) -> i32 { *x as i32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5117_deref_narrowing_still_flagged() {
        // `*x as i8` where `*x: u16` discards the high byte — genuinely lossy.
        // A deref operand parses as a `unary_expression`, which
        // `rust-no-as-numeric-cast` treats as a literal cast and never owns, so
        // this rule is the sole owner and must flag it.
        let src = "fn p(x: &u16) -> i8 { *x as i8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5117_deref_sign_loss_still_flagged() {
        // `*x as u16` where `*x: i32` narrows 32→16 bits — genuinely lossy, and
        // this rule owns the deref span (see above), so it must flag it.
        let src = "fn p(x: &i32) -> u16 { *x as u16 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5162_deref_range_start_as_u32_not_flagged() {
        // Issue #5162: `RangeInclusive<char>::start()`/`.end()` return `&char`;
        // `*range.start()` derefs to `char`, and `char as u32` is lossless.
        let src = "fn f(range: std::ops::RangeInclusive<char>) -> u32 { *range.start() as u32 }";
        assert!(run_on(src).is_empty());
        let src_end = "fn f(range: std::ops::RangeInclusive<char>) -> u32 { *range.end() as u32 }";
        assert!(run_on(src_end).is_empty());
    }

    #[test]
    fn repro_5162_deref_range_start_as_u8_still_flagged() {
        // Narrowing guard: `*range.start() as u8` truncates a char (21 bits) to a
        // byte — `char_fits(u8)` is false, so the char carve-out does not apply
        // and the genuine lossy cast is still flagged. Proves the deref/char
        // detection only exempts wide-enough targets, not narrowing ones.
        let src = "fn f(range: std::ops::RangeInclusive<char>) -> u8 { *range.start() as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn char_literal_as_u32_not_flagged() {
        // `'a' as u32` is lossless and total.
        assert!(run_on("fn f() -> u32 { 'a' as u32 }").is_empty());
    }

    #[test]
    fn genuine_lossy_int_as_f32_still_flagged() {
        // Sanity guard that the char carve-out did not over-exempt: a real
        // unsigned widening to f32 (which `rust-no-as-numeric-cast` skips) still
        // fires here.
        assert_eq!(run_on("fn f(x: u32) -> f32 { x as f32 }").len(), 1);
    }

    #[test]
    fn repro_5033_byte_extraction_shift_not_flagged() {
        // `(bits >> 32) as u8` — high-bits-cleared byte extraction (HPACK
        // Huffman encoder pattern). The cast is deliberate bit manipulation;
        // `rust-no-as-numeric-cast` no longer owns the span (it exempts
        // bitwise operands), so this rule must not flag it either.
        assert!(run_on("fn f(bits: u64) -> u8 { (bits >> 32) as u8 }").is_empty());
        assert!(run_on("fn f(x: u32) -> u8 { (x >> 24) as u8 }").is_empty());
    }

    #[test]
    fn repro_5034_assert_max_bound_not_flagged() {
        // The issue's shape: an `assert!(x <= u8::MAX as u64)` proves the value
        // fits the target. `rust-no-as-numeric-cast` exempts it too, so the pair
        // stays silent.
        let src = "fn f(x: u64) -> u8 { assert!(x <= u8::MAX as u64); let y = x as u8; y }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5034_assert_literal_bound_not_flagged() {
        let src = "fn g(n: u64) -> u8 { assert!(n < 256); n as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5034_no_assert_owned_by_numeric_cast() {
        // No assert — a real narrowing, but `rust-no-as-numeric-cast` owns the
        // span, so this rule suppresses.
        assert!(run_on("fn f(n: u64) -> u8 { n as u8 }").is_empty());
    }

    #[test]
    fn repro_5034_assert_on_different_variable_owned_by_numeric_cast() {
        // The assert bounds `m`, not the cast operand `n`; the narrowing stays a
        // finding, owned by `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(n: u64, m: u64) -> u8 { assert!(m <= 255); n as u8 }").is_empty());
    }
}
