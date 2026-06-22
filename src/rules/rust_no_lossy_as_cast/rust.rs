//! rust-no-lossy-as-cast backend.
//!
//! Walks `type_cast_expression` nodes (the `expr as Type` syntax)
//! and flags casts where the destination type is in our "narrowing
//! or precision-losing" set:
//!
//! - integer narrowing (`u32 as u8`, `i64 as i32`, etc.)
//! - float to integer (`f64 as u32`)
//!
//! Integer -> float casts (`x as f32` / `x as f64`) are not flagged when the
//! operand resolves to an integer type: a lossy int -> float conversion has no
//! `From`/`TryFrom` alternative in std (the trait impls exist only for the
//! lossless pairs), so `as` is the only conversion the language offers and a
//! `try_into()` / `From` suggestion would not compile. An operand whose source
//! type can't be resolved from the AST (index/field/method operands, e.g.
//! `gx[(x, y)][0] as f32`) is left to `rust-no-as-numeric-cast`.
//!
//! Same-width signed/unsigned reinterpretations (`u8 as i8`, `i32 as u32`,
//! …) preserve every bit — only the sign bit's interpretation changes via
//! two's complement — so they are not lossy and are silenced when the
//! source type is locally visible.
//!
//! A cast feeding a `from_bits` call — `f32::from_bits(p as u32)`,
//! `f64::from_bits(x as u64)` — is exempt regardless of the operand's
//! resolvability: `from_bits` reinterprets raw bits, so the `as` adapting the
//! operand to its parameter type (e.g. the `i32` the x86 `_mm_extract_ps`
//! intrinsic returns, cast to the `u32` `f32::from_bits` expects) is a
//! bit-preserving reinterpretation, and a `try_from` would reject valid
//! negative bit patterns.
//!
//! Likewise, a same-width signed↔unsigned cast feeding an x86 SIMD intrinsic
//! argument — `_mm_set1_epi32(x as i32)` where `x: u32` — is exempt: Intel's
//! intrinsics type integer lanes as signed (the C ABI), so passing a `u32` bit
//! pattern requires a same-width `as i32` reinterpretation that preserves every
//! bit.
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
//! A `char as <int>` cast gated by an `is_ascii()` / `is_ascii_*()` check on the
//! same value — `self.is_ascii().then_some(*self as u8)` or `if ch.is_ascii() {
//! ch as u8 }` — is exempt: an ASCII char is `0..=127`, which fits any integer
//! at least 8 bits wide, so the guarded cast cannot truncate.
//!
//! Casts that `rust-no-as-numeric-cast` already flags are suppressed here so
//! the pair emits one diagnostic per span.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    cast_feeds_from_bits, cast_feeds_simd_intrinsic, cast_feeds_sized_pointer_write,
    cast_in_const_context, cast_is_int_to_float, cast_operand_bit_width,
    cast_operand_indexed_element_type,
    cast_operand_is_ascii_guarded, cast_operand_is_assert_bounded, cast_operand_is_bitwise,
    cast_operand_is_bool, cast_operand_is_char, cast_operand_is_collection_size,
    cast_operand_is_enum_discriminant, cast_operand_is_non_negative_guarded,
    cast_operand_is_range_guarded, cast_operand_is_repr_enum_field, cast_operand_literal_value,
    find_identifier_type, is_in_enum_discriminant,
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
        if cast_in_const_context(node, source_bytes) {
            return;
        }
        if cast_operand_is_char(node, source_bytes) && char_fits(target_type) {
            return;
        }
        if cast_operand_is_ascii_guarded(node, source_bytes) {
            return;
        }
        if cast_operand_literal_value(node, source_bytes)
            .is_some_and(|value| literal_fits(value, target_type))
        {
            return;
        }
        if cast_operand_bit_width(node, source_bytes)
            .is_some_and(|bits| bit_width_fits(bits, target_type))
        {
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
        if cast_operand_is_repr_enum_field(node, source_bytes, target) {
            return;
        }
        if cast_operand_is_range_guarded(node, source_bytes) {
            return;
        }
        if cast_operand_is_non_negative_guarded(node, source_bytes) {
            return;
        }
        if cast_operand_is_assert_bounded(node, source_bytes) {
            return;
        }
        if cast_operand_is_bitwise(node, source_bytes) {
            return;
        }
        if cast_feeds_from_bits(node, source_bytes) {
            return;
        }
        if cast_feeds_simd_intrinsic(node, source_bytes) {
            return;
        }
        if cast_feeds_sized_pointer_write(node, source_bytes) {
            return;
        }
        // An integer -> float cast (`x as f32` / `x as f64`) has no lossless
        // trait alternative: `From`/`TryFrom` for int -> float exist only for
        // the lossless pairs, and for the lossy pairs (`i64`/`u64`/… -> `f64`,
        // `i32`/`u32`/… -> `f32`) `as` is the only conversion the language
        // offers. Suggesting `try_into()` / `From::from(x)` there is impossible,
        // so exempt the cast once the operand is a resolved integer.
        if cast_is_int_to_float(node, source_bytes) {
            return;
        }
        let source_type = source_numeric_type(node, source_bytes);
        if let Some(source_type) = source_type
            && !is_dangerous_cast(source_type, target_type)
        {
            return;
        }
        // A resolved-integer `as f32` was exempted above; what reaches here with
        // a float target is an unresolved operand (index/field/method, e.g.
        // `gx[(x, y)][0] as f32`), where precision loss is not provable, so defer
        // to `rust-no-as-numeric-cast`, which flags float casts only when a
        // matching `From` impl makes `T::from(x)` a compiling suggestion.
        if target_type.kind == NumericKind::Float && source_type.is_none() {
            return;
        }
        if is_in_enum_discriminant(node) {
            return;
        }
        // De-duplicate: when `rust-no-as-numeric-cast` owns this cast, let it
        // be the single diagnostic for the span. This rule keeps firing only
        // where that one does not (e.g. `f64 as f32` float narrowing).
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
    true
}

/// A `char` is a Unicode scalar value in `0..=0x10FFFF` (21 bits), so a cast
/// to any signed/unsigned integer of at least 21 bits is lossless. Floats are
/// excluded — the rule never claims a float target is safe here.
fn char_fits(target: NumericType) -> bool {
    target.kind != NumericKind::Float && target.bits >= 21
}

/// True if an `N`-bit value (from a bit-reader `read_bits(N)` operand) fits
/// losslessly into `target`. An unsigned `uM` holds any `N`-bit value when
/// `N <= M`; a signed `iM` reserves one bit for the sign, so it holds an
/// (unsigned) `N`-bit value only when `N <= M - 1`. Floats are excluded — a
/// bit-reader value is an integer, never a float target here.
fn bit_width_fits(read_bits: u16, target: NumericType) -> bool {
    match target.kind {
        NumericKind::Unsigned => read_bits <= target.bits,
        NumericKind::Signed => read_bits < target.bits,
        NumericKind::Float => false,
    }
}

/// True if the integer `value` (parsed from a literal operand) lies within the
/// inclusive `[MIN, MAX]` range of the integer `target`, making the cast
/// lossless — e.g. `b' ' as i8` (the byte 32 fits `-128..=127`). The rule's
/// `NARROWING_TARGETS` set never includes a float or platform-width type, so
/// `target_int_bounds` always resolves here.
fn literal_fits(value: i128, target: NumericType) -> bool {
    let Some((min, max)) = target_int_bounds(target) else {
        return false;
    };
    value >= min && value <= max
}

/// The inclusive `[MIN, MAX]` bounds of an integer `target` as `i128`, or `None`
/// for a float target. Shifts are checked to stay within `i128`.
fn target_int_bounds(target: NumericType) -> Option<(i128, i128)> {
    match target.kind {
        NumericKind::Float => None,
        NumericKind::Unsigned => {
            let max = 1i128.checked_shl(u32::from(target.bits)).map_or(i128::MAX, |p| p - 1);
            Some((0, max))
        }
        NumericKind::Signed => {
            let max = 1i128
                .checked_shl(u32::from(target.bits - 1))
                .map_or(i128::MAX, |p| p - 1);
            let min = max.checked_neg().and_then(|n| n.checked_sub(1)).unwrap_or(i128::MIN);
            Some((min, max))
        }
    }
}

fn source_numeric_type(node: tree_sitter::Node, source: &[u8]) -> Option<NumericType> {
    let value = node.child_by_field_name("value")?;
    let ident = deref_identifier(value, source).unwrap_or(value);
    if ident.kind() != "identifier" {
        // `base[idx] as T` where `base` is a locally-declared slice/array/Vec of
        // a fixed-width integer (`buf: &[u8; N]`): the element type is the cast's
        // source, so `buf[0] as u32` is a provable widening.
        let element_type = cast_operand_indexed_element_type(node, source)?;
        return numeric_type(&element_type);
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
    fn allows_relocation_sized_pointer_write() {
        // Issue #5677: a value cast whose width matches the destination
        // pointer's pointee width is a deliberate store-width truncation.
        assert!(
            run_on("fn f() { unsafe { write_unaligned(addr as *mut u8, abs as u8); } }").is_empty()
        );
        assert!(
            run_on("fn f() { unsafe { write_unaligned(addr as *mut u16, abs as u16); } }")
                .is_empty()
        );
    }

    #[test]
    fn allows_lossy_cast_in_const_initializer() {
        // In a const-evaluation context `try_into()` is unavailable and `as`
        // is the only conversion, so a lossy const cast is exempt (#5679).
        assert!(run_on("const X: f32 = i32::MAX as f32;").is_empty());
    }

    #[test]
    fn allows_lossy_cast_in_static_initializer() {
        assert!(run_on("static S: f32 = i32::MAX as f32;").is_empty());
    }

    #[test]
    fn allows_lossy_cast_in_const_fn_body() {
        assert!(run_on("const fn f(x: i32) -> f32 { x as f32 }").is_empty());
    }

    #[test]
    fn repro_5690_int_to_f32_in_non_const_fn_body_not_flagged() {
        // Issue #5690: `i32 as f32` has no `f32::From<i32>` / `TryFrom` — `as` is
        // the only conversion, so a runtime body is exempt too (not just const).
        assert!(run_on("fn f(x: i32) -> f32 { x as f32 }").is_empty());
    }

    #[test]
    fn float_to_int_owned_by_numeric_cast() {
        // The inverse direction (`f64 as i32`) is not exempt: `try_into()` /
        // `i32::try_from` exist for float -> int, so the cast stays a finding —
        // here owned by `rust-no-as-numeric-cast`, so this rule cedes the span.
        assert!(run_on("fn f(x: f64) -> i32 { x as i32 }").is_empty());
    }

    #[test]
    fn repro_5690_int_narrowing_owned_by_numeric_cast() {
        // The int -> float exemption must NOT bleed into int -> int narrowing:
        // `i64 as i32` has a `try_into()` alternative, so it stays a finding —
        // here owned by `rust-no-as-numeric-cast`, so this rule cedes the span.
        assert!(run_on("fn f(x: i64) -> i32 { x as i32 }").is_empty());
    }

    #[test]
    fn repro_5690_deref_int_narrowing_still_flagged() {
        // A deref int narrowing is owned by this rule (numeric-cast cedes deref
        // operands): `*x as i32` where `x: &i64` must still flag — the int -> int
        // direction is never exempt.
        assert_eq!(run_on("fn f(x: &i64) -> i32 { *x as i32 }").len(), 1);
    }

    #[test]
    fn repro_5690_deref_float_to_int_still_flagged() {
        // A deref operand is ceded by `rust-no-as-numeric-cast`, so this rule
        // owns the span. `*x as i32` where `x: &f64` is a float -> int cast — the
        // int-to-float exemption must NOT apply to the reverse direction.
        assert_eq!(run_on("fn f(x: &f64) -> i32 { *x as i32 }").len(), 1);
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
    fn repro_5259_u8_as_i32_not_flagged() {
        // Issue #5259 (jpeg-decoder ycbcr_to_rgb): `u8 as i32` is unsigned ->
        // strictly wider signed — every u8 (0..=255) fits in i32, so lossless.
        assert!(run_on("fn f(y: u8) -> i32 { y as i32 }").is_empty());
    }

    #[test]
    fn repro_5259_u8_as_i64_not_flagged() {
        assert!(run_on("fn f(y: u8) -> i64 { y as i64 }").is_empty());
    }

    #[test]
    fn repro_5259_u16_as_i64_not_flagged() {
        assert!(run_on("fn f(y: u16) -> i64 { y as i64 }").is_empty());
    }

    #[test]
    fn repro_5259_u16_as_i8_owned_by_numeric_cast() {
        // Unsigned -> narrower signed: u16 max 65535 > i8 max 127 — genuinely
        // lossy, NOT exempted by the widening carve-out. `rust-no-as-numeric-cast`
        // owns the span, so this rule suppresses its diagnostic.
        assert!(run_on("fn f(x: u16) -> i8 { x as i8 }").is_empty());
    }

    #[test]
    fn repro_5259_i32_as_u32_signed_to_unsigned_unchanged() {
        // Signed -> same-width unsigned stays a bit reinterpretation (negative
        // values reinterpret); `rust-no-as-numeric-cast` owns the span.
        assert!(run_on("fn f(x: i32) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn is_dangerous_cast_unsigned_to_signed_boundary() {
        let u = |bits| NumericType { kind: NumericKind::Unsigned, bits };
        let i = |bits| NumericType { kind: NumericKind::Signed, bits };
        // uN -> iM lossless iff M > N (strictly wider signed).
        assert!(!is_dangerous_cast(u(8), i(16)));
        assert!(!is_dangerous_cast(u(8), i(32)));
        assert!(!is_dangerous_cast(u(16), i(32)));
        // M == N: same-width reinterpretation, also exempt (issue #4807).
        assert!(!is_dangerous_cast(u(16), i(16)));
        assert!(!is_dangerous_cast(u(32), i(32)));
        // M < N: unsigned -> narrower signed is genuinely lossy.
        assert!(is_dangerous_cast(u(16), i(8)));
        assert!(is_dangerous_cast(u(32), i(16)));
        // Signed -> unsigned is unchanged (existing behavior).
        assert!(!is_dangerous_cast(i(32), u(32))); // same-width reinterpret
        assert!(is_dangerous_cast(i(32), u(16))); // narrowing
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
    fn repro_5122_is_ascii_then_some_char_as_u8_not_flagged() {
        // Issue #5122 (chumsky): `is_ascii()` proves `*self` is in `0..=127`, so
        // the `*self as u8` it gates is lossless.
        let src = "fn to_ascii(&self) -> Option<u8> { self.is_ascii().then_some(*self as u8) }";
        assert!(run_on(src).is_empty());
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
    fn repro_5690_u32_as_f32_not_flagged() {
        // `y as f32` where `y: u32`: no `f32::From<u32>` / `TryFrom` exists, so
        // `as` is the only conversion — neither rule flags it (#5690).
        assert!(run_on("fn f(y: u32) -> f32 { let x = y as f32; x }").is_empty());
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
    fn repro_5690_u64_as_f32_not_flagged() {
        // The issue's exact shape (cranelift opts.rs `f32_from_uint`):
        // `n as f32` where `n: u64`. No `f32::From<u64>` / `TryFrom` exists.
        assert!(run_on("fn f32_from_uint(n: u64) -> f32 { n as f32 }").is_empty());
    }

    #[test]
    fn repro_5690_i64_as_f32_not_flagged() {
        // `f32_from_sint`: `n as f32` where `n: i64`.
        assert!(run_on("fn f32_from_sint(n: i64) -> f32 { n as f32 }").is_empty());
    }

    #[test]
    fn repro_5690_u64_as_f64_not_flagged() {
        // `f64_from_uint`: `n as f64` where `n: u64`. `f64` is not even in the
        // narrowing-target set, but the int -> float exemption covers it too.
        assert!(run_on("fn f64_from_uint(n: u64) -> f64 { n as f64 }").is_empty());
    }

    #[test]
    fn repro_5690_i64_as_f64_not_flagged() {
        assert!(run_on("fn f64_from_sint(n: i64) -> f64 { n as f64 }").is_empty());
    }

    #[test]
    fn repro_5690_usize_as_f64_not_flagged() {
        // A platform-width source (`usize`) has no `f64::From<usize>` either.
        assert!(run_on("fn f(n: usize) -> f64 { n as f64 }").is_empty());
    }

    #[test]
    fn repro_5690_i32_as_f32_not_flagged() {
        // i32 exceeds f32's 24-bit mantissa (lossy), but no `f32::From<i32>` /
        // `TryFrom` exists, so `as` is the only conversion — not flagged (#5690).
        assert!(run_on("fn f(g: i32) -> f32 { g as f32 }").is_empty());
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
    fn repro_5690_u32_as_f32_after_char_carveout_not_flagged() {
        // The char carve-out does not change the int -> f32 verdict: `u32 as f32`
        // has no `From`/`TryFrom` alternative, so it is exempt (#5690).
        assert!(run_on("fn f(x: u32) -> f32 { x as f32 }").is_empty());
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

    #[test]
    fn repro_5257_read_bits_within_target_not_flagged() {
        // A bit-reader reading N <= target bits is lossless — codec idiom.
        assert!(run_on("fn f(r: R) -> u8 { r.read_bits(4) as u8 }").is_empty());
        assert!(run_on("fn f(r: R) -> u8 { r.read_bits(8) as u8 }").is_empty());
        assert!(run_on("fn f(bs: B) -> u16 { bs.read_bits_leq32(16)? as u16 }").is_empty());
        assert!(run_on("fn f(r: R) -> u8 { r.get_bits(2)? as u8 }").is_empty());
        // 7 bits fit a signed `i8`.
        assert!(run_on("fn f(r: R) -> i8 { r.read_bits(7) as i8 }").is_empty());
    }

    #[test]
    fn repro_5257_oversized_read_bits_owned_by_numeric_cast() {
        // Reading more bits than the target holds is a genuine narrowing, owned
        // by `rust-no-as-numeric-cast`, so this rule suppresses on the span.
        assert!(run_on("fn f(r: R) -> u8 { r.read_bits(9) as u8 }").is_empty());
        // `i8` reserves the sign bit: an 8-bit read does not fit.
        assert!(run_on("fn f(r: R) -> i8 { r.read_bits(8) as i8 }").is_empty());
    }

    #[test]
    fn repro_5257_non_literal_count_owned_by_numeric_cast() {
        // A non-literal count is not statically bounded; the narrowing stays a
        // finding, owned by `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(r: R, n: u32) -> u8 { r.read_bits(n) as u8 }").is_empty());
    }

    #[test]
    fn repro_5260_self_repr_enum_field_as_u8_not_flagged() {
        // The issue's shape (image-png common.rs): `self.dispose_op as u8` where
        // `dispose_op: DisposeOp` and `DisposeOp` is `#[repr(u8)]`. The repr
        // guarantees every discriminant fits u8, so the cast is lossless.
        let src = "#[repr(u8)] enum DisposeOp { None = 0, Background = 1, Previous = 2 } \
                   struct Frame { dispose_op: DisposeOp } \
                   impl Frame { fn ser(&self) -> u8 { self.dispose_op as u8 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_param_repr_enum_field_as_u8_not_flagged() {
        // The issue's other shape (stream.rs): `frame.dispose_op as u8` where
        // `frame: &FrameControl` is a parameter and the field type is `#[repr(u8)]`.
        let src = "#[repr(u8)] enum DisposeOp { None, Background } \
                   struct FrameControl { dispose_op: DisposeOp } \
                   fn ser(frame: &FrameControl) -> u8 { frame.dispose_op as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_repr_u8_enum_field_to_wider_u16_not_flagged() {
        // Widening a repr(u8) enum to u16 is also lossless.
        let src = "#[repr(u8)] enum E { A } \
                   struct S { f: E } \
                   fn g(s: &S) -> u16 { s.f as u16 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_direct_variant_as_u8_not_flagged() {
        // A direct `EnumName::Variant as u8` of a repr(u8) enum (already covered
        // by the discriminant helper) stays silent.
        let src = "#[repr(u8)] enum E { A = 0, B = 1 } fn f() -> u8 { E::B as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_repr_u16_enum_field_as_u8_owned_by_numeric_cast() {
        // A `#[repr(u16)]` enum can hold discriminants up to 65535, which do not
        // fit u8 — the repr-enum exemption must NOT apply. The narrowing stays a
        // finding (here owned by `rust-no-as-numeric-cast`).
        let src = "#[repr(u16)] enum E { A } \
                   struct S { f: E } \
                   fn g(s: &S) -> u8 { s.f as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_non_repr_enum_field_as_u8_owned_by_numeric_cast() {
        // Without `#[repr(intN)]` the discriminant width is not guaranteed, so
        // the exemption must not apply; the narrowing stays a finding.
        let src = "enum E { A } \
                   struct S { f: E } \
                   fn g(s: &S) -> u8 { s.f as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_integer_field_narrowing_owned_by_numeric_cast() {
        // The cast must NOT be un-flagged when the field is a wider integer, not
        // a repr-enum: `u16` field `as u8` is genuinely lossy. The repr-enum
        // exemption does not apply; `rust-no-as-numeric-cast` owns the span (the
        // field operand's source type is unresolved), so this rule suppresses.
        let src = "struct S { count: u16 } \
                   fn g(s: &S) -> u8 { s.count as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5209_byte_literal_as_i8_not_flagged() {
        // The issue's shape: `b' ' as i8` (WinAPI `CHAR`). Byte 32 fits i8, so
        // this rule exempts it directly (not merely via numeric-cast dedup).
        assert!(run_on("fn f() -> i8 { b' ' as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { b'A' as i8 }").is_empty());
    }

    #[test]
    fn repro_5209_int_literal_in_range_not_flagged() {
        assert!(run_on("fn f() -> i8 { 0x41 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 65 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { -5 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 0o17 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 0b0101 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 65u8 as i8 }").is_empty());
    }

    #[test]
    fn repro_5209_positive_out_of_range_literal_owned_by_numeric_cast() {
        // 200 > i8::MAX: a genuine lossy literal cast. `rust-no-as-numeric-cast`
        // owns the span (it flags out-of-range integer literals), so this rule
        // suppresses — the pair still emits exactly one diagnostic.
        assert!(run_on("fn f() -> i8 { 200 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 0xFF as i8 }").is_empty());
        assert!(run_on("fn f() -> u8 { 300 as u8 }").is_empty());
    }

    #[test]
    fn repro_5209_negative_out_of_range_literal_still_flagged() {
        // -200 < i8::MIN: a negated literal is a `unary_expression`, which
        // `rust-no-as-numeric-cast` cedes, so this rule is the sole owner and
        // must flag the out-of-range value.
        assert_eq!(run_on("fn f() -> i8 { -200 as i8 }").len(), 1);
    }

    #[test]
    fn repro_5209_non_literal_operand_owned_by_numeric_cast() {
        // A variable operand is not a literal; the narrowing stays a finding,
        // owned by `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(x: i32) -> i8 { x as i8 }").is_empty());
    }

    #[test]
    fn repro_5262_non_negative_guarded_signed_to_unsigned_not_flagged() {
        // `i8 >= 0` proves the value is non-negative, so the signed→unsigned
        // (equal-width) cast `i8 as u8` is lossless. Both rules exempt it.
        assert!(run_on("fn f(x: i8) -> u8 { if x >= 0 { x as u8 } else { 0 } }").is_empty());
    }

    #[test]
    fn repro_5262_match_guard_widening_signed_to_unsigned_not_flagged() {
        // `i16 >= 0` widening to u32 is lossless; the match-arm guard is the
        // proof site.
        let src = "fn f(o: Option<i16>) -> Option<u32> { \
                   match o { Some(v) if v >= 0 => Some(v as u32), _ => None } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5262_unguarded_signed_to_unsigned_owned_by_numeric_cast() {
        // No guard: the cast is a finding, owned by `rust-no-as-numeric-cast`,
        // so this rule cedes the span and stays empty.
        assert!(run_on("fn f(x: i16) -> u8 { x as u8 }").is_empty());
    }

    #[test]
    fn repro_5318_byte_slice_index_as_u32_not_flagged() {
        // `buf: &[u8]`, so `buf[0]` is u8 and `buf[0] as u32` is widening — the
        // resolved element type proves the cast is safe.
        assert!(run_on("fn f(buf: &[u8]) -> u32 { buf[0] as u32 }").is_empty());
    }

    #[test]
    fn repro_5318_byte_array_index_as_u32_not_flagged() {
        assert!(run_on("fn f(buf: &[u8; 8]) -> u32 { buf[0] as u32 }").is_empty());
    }

    #[test]
    fn repro_5318_narrowing_index_cast_owned_by_numeric_cast() {
        // `buf: &[u32]`, target u8 — a genuine narrowing. The element type now
        // resolves, so the cast is a real finding; `rust-no-as-numeric-cast`
        // owns the span, so this rule cedes it and stays empty.
        assert!(run_on("fn f(buf: &[u32]) -> u8 { buf[0] as u8 }").is_empty());
    }

    #[test]
    fn repro_5318_unresolvable_base_index_cast_owned_by_numeric_cast() {
        // The base element type is unresolvable (method return); the narrowing
        // stays a finding, owned by `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(thing: T) -> u8 { thing.bytes()[0] as u8 }").is_empty());
    }

    #[test]
    fn repro_5469_suffixed_literal_u64_as_u32_not_flagged() {
        // Issue #5469 (wasm-bindgen web-sys WebGL constants): `NNNu64 as u32`
        // where the operand is a suffixed integer literal whose value fits u32.
        // The literal value is statically known and in range, so the cast is
        // provably lossless.
        assert!(run_on("pub const DEPTH: u32 = 256u64 as u32;").is_empty());
        assert!(run_on("pub const STENCIL: u32 = 1024u64 as u32;").is_empty());
        assert!(run_on("pub const COLOR: u32 = 16384u64 as u32;").is_empty());
        // Hex form, the other WebGL constant shape (`0x1F00u64 as u32`).
        assert!(run_on("pub const TEXTURE: u32 = 0x1F00u64 as u32;").is_empty());
    }

    #[test]
    fn repro_5469_out_of_range_suffixed_literal_owned_by_numeric_cast() {
        // A suffixed literal whose value exceeds the target is genuinely lossy.
        // `rust-no-as-numeric-cast` owns the span (it flags out-of-range integer
        // literals), so this rule suppresses — the pair emits one diagnostic.
        assert!(run_on("fn f() -> u8 { 300u64 as u8 }").is_empty());
        assert!(run_on("fn f() -> u16 { 0x1_0000u64 as u16 }").is_empty());
    }

    #[test]
    fn repro_5593_from_bits_arg_i32_as_u32_not_flagged() {
        // Issue #5593 (qdrant simple_avx.rs): `f32::from_bits(p1 as u32)` where
        // `p1` is the i32 the `_mm_extract_ps` intrinsic returns (unresolvable
        // source). The cast feeds a bit-reinterpretation sink, so it is the
        // correct same-width tool — `u32::try_from` would reject negative bits.
        let src = "fn f() -> f32 { let p1 = _mm_extract_ps(h, 0); f32::from_bits(p1 as u32) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5593_f64_from_bits_arg_i64_as_u64_not_flagged() {
        let src = "fn f() -> f64 { let p = q(); f64::from_bits(p as u64) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5593_genuine_lossy_cast_outside_from_bits_still_flagged() {
        // A real narrowing not feeding `from_bits` keeps flagging: the exemption
        // is scoped to the bit-reinterpretation sink.
        assert_eq!(run_on("fn f(x: f64) -> f32 { x as f32 }").len(), 1);
    }

    #[test]
    fn repro_5550_external_repr_enum_param_as_u32_not_flagged() {
        // The issue's exact shape (naga SPIR-V backend): a parameter typed as an
        // imported `#[repr(u32)]` enum cast to u32. The repr guarantees every
        // discriminant fits u32, so the cast is lossless; `as` is the only
        // conversion the language offers.
        let src =
            "fn source(source_language: spirv::SourceLanguage) -> u32 { source_language as u32 }";
        assert!(run_on(src).is_empty());
        let src2 = "fn decorate(decoration: spirv::Decoration) -> u32 { decoration as u32 }";
        assert!(run_on(src2).is_empty());
    }

    #[test]
    fn repro_5550_bare_type_name_binding_owned_by_numeric_cast() {
        // A bare (unqualified) PascalCase type name is indistinguishable from a
        // local numeric alias, so the discriminant exemption must NOT apply. The
        // narrowing stays a finding — owned by `rust-no-as-numeric-cast`, so this
        // rule cedes the span.
        let src = "fn f(d: Decoration) -> u8 { d as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5550_numeric_param_narrowing_owned_by_numeric_cast() {
        // A genuinely numeric operand is NOT enum-shaped: `u64 as u32` stays a
        // lossy narrowing, owned by `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(x: u64) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn repro_5600_same_width_cast_feeding_simd_intrinsic_not_flagged() {
        // Issue #5600: a same-width signed↔unsigned cast feeding an x86 SIMD
        // intrinsic is a bit reinterpretation, not a lossy narrowing — exempt in
        // both rules. `u32 as i32` (an in-scope signed target here) feeding
        // `_mm_set1_epi32`, with `x: u32` resolvable.
        assert!(run_on("fn f(x: u32) -> __m128i { _mm_set1_epi32(x as i32) }").is_empty());
    }

    #[test]
    fn repro_5600_unresolved_source_same_width_simd_not_flagged() {
        // Unresolved source (call return) cast to the intrinsic's lane type:
        // `load() as i32` feeding `_mm_set1_epi32`. The lane width (epi32 = 32)
        // matches the target, so the cast is the genuine lane reinterpretation.
        // Without the SIMD anchor this would leak through (numeric-cast no longer
        // owns it), so this rule must exempt it too.
        assert!(run_on("fn f() -> __m128i { _mm_set1_epi32(load() as i32) }").is_empty());
    }

    #[test]
    fn repro_5600_narrowing_into_simd_intrinsic_owned_by_numeric_cast() {
        // A genuinely narrowing cast feeding a SIMD intrinsic is NOT same-width,
        // so the anchor must not exempt it: `u64 as i32` discards 32 bits. The
        // narrowing stays a finding, owned by `rust-no-as-numeric-cast`.
        assert!(run_on("fn f(x: u64) -> __m128i { _mm_set1_epi32(x as i32) }").is_empty());
    }
}
