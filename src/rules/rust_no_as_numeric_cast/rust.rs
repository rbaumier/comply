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
//! `proc-macro = true` crates are exempted wholesale: their `as` casts operate on
//! compile-time AST/codegen quantities (token indices, field counts) that are
//! bounded and tiny, not runtime data, so the lossy-truncation concern does not
//! apply, and `as` is the idiomatic way to feed syn/quote constructors
//! (`syn::Index.index` is a `u32`).
//!
//! Non-numeric targets (pointer, reference, trait object) are ignored.
//!
//! A raw-pointer-to-integer cast is exempt: when the operand is a raw pointer —
//! an inner `<expr> as *const/*mut T` cast, a `.as_ptr()`/`.as_mut_ptr()` call,
//! or a `ptr::null()`/`null_mut()` call — `as <int>` is the only conversion (no
//! `From`/`TryFrom` exists for `*const T`/`*mut T` to an integer), so the rule's
//! `from`/`try_from` remediation is inapplicable. This is the embedded idiom for
//! passing a memory-mapped register / DMA buffer address to a hardware register
//! (`task.as_ptr() as u32`, `&mut self.table as *mut _ as *mut u32 as u32`).
//!
//! Casts whose operand's outermost expression is a bitwise op
//! (`>>`, `<<`, `&`, `|`, `^`, parens transparent) are bit manipulation —
//! e.g. `(x >> 8) as u8`, `(x & 0xFF) as u8`. The truncation is intentional,
//! so `try_from` would be wrong; these are left alone.
//!
//! A cast feeding a `from_bits` call — `f32::from_bits(p as u32)`,
//! `f64::from_bits(x as u64)` — is exempt: `from_bits` reinterprets raw bits,
//! so the `as` adapting the operand to its parameter type (e.g. the `i32` the
//! x86 `_mm_extract_ps` intrinsic returns, cast to the `u32` `f32::from_bits`
//! expects) is a bit-preserving reinterpretation, and a `try_from` would
//! reject valid negative bit patterns.
//!
//! A same-width signed↔unsigned cast feeding an x86 SIMD intrinsic argument —
//! `_mm_set_epi64x(hi as i64, lo as i64)` where `hi`/`lo` are `u64` — is exempt:
//! Intel's intrinsics type integer lanes as signed (the C ABI), so passing a
//! `u64` bit pattern requires a same-width `as i64` reinterpretation, where
//! `try_from` would reject bit patterns above `i64::MAX`.
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
//! A cast from a resolved `usize`/`isize` operand into a same-signedness
//! fixed-width integer of at least 64 bits — `usize as u64`/`u128`,
//! `isize as i64`/`i128` — is exempt: `usize`/`isize` are at most 64 bits on
//! every supported target, so the cast is always lossless, and no
//! `From<usize>`/`From<isize>` impl exists for a fixed-width integer, so `as`
//! is the only infallible conversion. Narrowing (`usize as u32`) and
//! sign-changing (`usize as i64`) casts stay flagged.
//!
//! A cast into `u128` from any non-float source — `u64 as u128`,
//! `entity.to_bits() as u128` — is exempt: `u128` is the widest integer type, so
//! no integer value of any width or signedness can overflow it, and there is no
//! `From` impl reachable for an unresolved source (method call / field access),
//! making the `u128::from(x)` remediation uncompilable. A float source
//! (`f64 as u128`) truncates and stays flagged.
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
use crate::rules::rust_helpers::{
    cast_feeds_from_bits, cast_feeds_simd_intrinsic, cast_feeds_sized_pointer_write,
    cast_in_const_context, cast_operand_bit_count_max, cast_operand_bit_width,
    cast_operand_indexed_element_type,
    cast_operand_is_ascii_guarded, cast_operand_is_assert_bounded, cast_operand_is_bitwise,
    cast_operand_is_bool, cast_operand_is_char, cast_operand_is_collection_size,
    cast_operand_is_enum_discriminant, cast_operand_is_min_clamped, cast_operand_is_modulo_bounded,
    cast_operand_is_non_negative_guarded,
    cast_operand_is_range_guarded, cast_operand_is_raw_pointer, cast_operand_is_repr_enum_field,
    cast_operand_is_sibling_arm_bounded, cast_operand_literal_value, find_identifier_type,
    is_in_enum_discriminant, is_in_test_context,
};

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
    /// `true` for `usize`/`isize`, whose width is platform-dependent. `bits`
    /// carries the host width for in-range checks, but two platform-width types
    /// (or a platform-width and a fixed-width type) cannot be proven "same width"
    /// across targets, so the same-width reinterpret carve-out must skip them.
    platform_width: bool,
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
        // A `proc-macro = true` crate's `as` casts operate on compile-time
        // AST/codegen quantities (token indices, field counts from `.enumerate()`
        // over struct fields) that are bounded and tiny, not runtime data. The
        // rule's lossy-truncation concern does not apply, and `as` is the
        // idiomatic way to feed syn/quote constructors (`syn::Index { index, .. }`
        // is a `u32`), where `try_from` would force an impossible error path.
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.is_proc_macro())
        {
            return;
        }
        if !fires_on_cast(node, source_bytes) {
            return;
        }
        let target = node
            .child_by_field_name("type")
            .and_then(|type_node| type_node.utf8_text(source_bytes).ok())
            .map_or("", str::trim);
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

/// Whether `rust-no-as-numeric-cast` flags this `type_cast_expression`.
///
/// Single source of truth for the rule's per-cast decision: a numeric target
/// that is not `usize`/`isize`, outside a test/enum-discriminant context, not a
/// literal/bitwise/collection-size/bool/enum-discriminant operand, and either a
/// dangerous integer cast or a `From`-available float cast.
///
/// `rust-no-lossy-as-cast` calls this to suppress its own diagnostic on casts
/// this rule already owns, so the pair emits one diagnostic per cast span.
pub(crate) fn fires_on_cast(node: tree_sitter::Node, source_bytes: &[u8]) -> bool {
    let Some(type_node) = node.child_by_field_name("type") else {
        return false;
    };
    let Ok(target_raw) = type_node.utf8_text(source_bytes) else {
        return false;
    };
    let target = target_raw.trim();
    let Some(target_type) = numeric_type(target) else {
        return false;
    };
    if target == "usize" || target == "isize" {
        return false;
    }
    if cast_operand_is_raw_pointer(node, source_bytes) {
        return false;
    }
    if is_in_test_context(node, source_bytes) {
        return false;
    }
    if is_in_enum_discriminant(node) {
        return false;
    }
    if cast_in_const_context(node, source_bytes) {
        return false;
    }
    if is_literal_cast(node, source_bytes) {
        return false;
    }
    if cast_operand_literal_value(node, source_bytes)
        .is_some_and(|value| literal_fits(value, target_type))
    {
        return false;
    }
    // A bit-counting method (`leading_zeros()`, `count_ones()`, …) returns a
    // value bounded by the receiver's bit-width — at most 128 — so casting it to
    // any integer that holds 128 is provably lossless.
    if cast_operand_bit_count_max(node, source_bytes)
        .is_some_and(|max| literal_fits(max, target_type))
    {
        return false;
    }
    if cast_operand_is_bitwise(node, source_bytes) {
        return false;
    }
    // `(x % N) as uT` with a non-negative `x` and `N - 1 <= uT::MAX` is in range
    // — the unsigned-remainder narrowing in `(width % 256) as u8` (#6151).
    if cast_operand_is_modulo_bounded(node, source_bytes) {
        return false;
    }
    // `<recv>.min(BOUND) as uT` where the `.min()` clamp proves the value fits the
    // unsigned target — `.as_nanos().min(u64::MAX as u128) as u64` (#6174).
    if cast_operand_is_min_clamped(node, source_bytes) {
        return false;
    }
    if cast_feeds_from_bits(node, source_bytes) {
        return false;
    }
    if cast_feeds_simd_intrinsic(node, source_bytes) {
        return false;
    }
    if cast_feeds_sized_pointer_write(node, source_bytes) {
        return false;
    }
    if cast_operand_is_collection_size(node, source_bytes) {
        return false;
    }
    if cast_operand_is_bool(node, source_bytes) {
        return false;
    }
    if cast_operand_is_char(node, source_bytes) && char_fits(target_type) {
        return false;
    }
    if cast_operand_is_ascii_guarded(node, source_bytes) {
        return false;
    }
    if cast_operand_bit_width(node, source_bytes)
        .is_some_and(|bits| bit_width_fits(bits, target_type))
    {
        return false;
    }
    if cast_operand_is_enum_discriminant(node, source_bytes) {
        return false;
    }
    if cast_operand_is_repr_enum_field(node, source_bytes, target) {
        return false;
    }
    if cast_operand_is_range_guarded(node, source_bytes) {
        return false;
    }
    if cast_operand_is_non_negative_guarded(node, source_bytes) {
        return false;
    }
    if cast_operand_is_assert_bounded(node, source_bytes) {
        return false;
    }
    // A guard-less wildcard `match` arm whose preceding sibling arms clamp every
    // out-of-range value (`val if val < 0 => 0, val if val > 0xFF => 0xFF, val =>
    // val as u8`) proves the cast operand fits the target exactly (#6150).
    if cast_operand_is_sibling_arm_bounded(node, source_bytes) {
        return false;
    }
    if cast_is_pointer_sized_widening(node, source_bytes, target) {
        return false;
    }
    let source_type = source_numeric_type(node, source_bytes);
    // `u128` is the widest integer type in Rust: no integer value — of any width
    // or signedness — can overflow it, so `<int> as u128` is always lossless
    // (`u64 as u128`, `entity.to_bits() as u128`). There is no `From<T>` impl for
    // `u128` from an unresolvable source (a method call / field access), so the
    // rule's `u128::from(x)` remediation would not compile. Only a float source
    // (`f64 as u128`) truncates, so it stays governed by `is_dangerous_cast`; an
    // unresolved source is treated as the common integer case and exempted. The
    // signed widest target `i128` is left to `is_dangerous_cast` — a `u128 as
    // i128` source can exceed `i128::MAX`, so the widest-target shortcut applies
    // only to the unsigned `u128` target.
    if target_type.kind == NumericKind::Unsigned
        && target_type.bits == 128
        && source_type.is_none_or(|src| src.kind != NumericKind::Float)
    {
        return false;
    }
    if target_type.kind == NumericKind::Float {
        // `as f32`/`as f64` is the only std conversion unless the source
        // has a matching `From` impl. Suggesting `f64::from(x)` for a
        // wider or unresolved source would not compile, so only flag
        // when `From` is provably available.
        source_type.is_some_and(|src| from_available(src, target_type))
    } else if let Some(src) = source_type {
        is_dangerous_cast(src, target_type)
    } else {
        true
    }
}

/// True when the operand's resolved source type is `usize`/`isize` and `target`
/// is a same-signedness fixed-width integer of at least 64 bits — `usize as
/// u64`/`u128`, `isize as i64`/`i128`.
///
/// Stable Rust guarantees `usize`/`isize` are at most 64 bits on every supported
/// target, so these casts are always lossless. There is no `From<usize>` /
/// `From<isize>` impl for any fixed-width integer (the width is platform-
/// dependent), so `as` is the only infallible conversion: the rule's
/// `u64::from(x)` suggestion would not compile and `u64::try_from(x)?` forces a
/// semantically-impossible error path. A narrowing (`usize as u32`), a
/// sign-changing (`usize as i64`), or a `From`-available cast resolves a
/// non-`usize`/`isize` source here and stays governed by `is_dangerous_cast`.
fn cast_is_pointer_sized_widening(node: tree_sitter::Node, source: &[u8], target: &str) -> bool {
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "identifier" {
        return false;
    }
    let Ok(name) = value.utf8_text(source) else {
        return false;
    };
    let Some(source_text) = find_identifier_type(node, name, source) else {
        return false;
    };
    matches!(
        (source_text.trim(), target),
        ("usize", "u64" | "u128") | ("isize", "i64" | "i128")
    )
}

fn numeric_type(type_text: &str) -> Option<NumericType> {
    let trimmed = type_text.trim();
    let (kind, bits) = match trimmed {
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
    Some(NumericType { kind, bits, platform_width: matches!(trimmed, "usize" | "isize") })
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
        // Same-width signed↔unsigned (`u8 as i8`, `i32 as u32`, …) preserves
        // every bit — only the sign bit's interpretation changes via two's
        // complement — so it is a lossless reinterpretation, not a narrowing.
        // `as` is the only conversion that performs it: there is no
        // `From`/`TryFrom` for the full range (`i8::from(u8)` does not exist,
        // and `200_u8.try_into::<i8>()` errors though the intended result is the
        // bit pattern `-56_i8`), so the rule's `from`/`try_from` remediation is
        // inapplicable. A `usize`/`isize` operand is excluded: its width is
        // platform-dependent, so "same width" as a fixed-width target cannot be
        // proven across targets (e.g. `usize as i64` is a sign change that widens
        // on a 32-bit target), and that case stays flagged.
        _ if source.bits == target.bits && !source.platform_width && !target.platform_width => {
            false
        }
        // Cross-signedness of different widths stays lossy: unsigned→narrower
        // signed (`u16 as i8`) discards bits, and signed→narrower unsigned
        // (`i32 as u16`) both narrows and may wrap a negative value.
        _ => source.bits >= target.bits,
    }
}

/// A `char` is a Unicode scalar value in `0..=0x10FFFF` (21 bits), so a cast to
/// any signed/unsigned integer of at least 21 bits is lossless. Floats are
/// excluded — `char as f32`/`f64` falls through to the float-target handling.
fn char_fits(target: NumericType) -> bool {
    target.kind != NumericKind::Float && target.bits >= 21
}

/// True if an `N`-bit value (from a bit-reader `read_bits(N)` operand) fits
/// losslessly into `target`. An unsigned `uM` holds any `N`-bit value when
/// `N <= M`; a signed `iM` reserves one bit for the sign, so it holds an
/// (unsigned) `N`-bit value only when `N <= M - 1`. Floats are excluded — a
/// bit-reader value is an integer, so `as f32`/`f64` falls through to the
/// float-target handling.
fn bit_width_fits(read_bits: u16, target: NumericType) -> bool {
    match target.kind {
        NumericKind::Unsigned => read_bits <= target.bits,
        NumericKind::Signed => read_bits < target.bits,
        NumericKind::Float => false,
    }
}

fn source_numeric_type(node: tree_sitter::Node, source: &[u8]) -> Option<NumericType> {
    let value = node.child_by_field_name("value")?;
    if value.kind() == "identifier" {
        let name = value.utf8_text(source).ok()?;
        let type_text = find_identifier_type(node, name, source)?;
        return numeric_type(&type_text);
    }
    // `base[idx] as T` where `base` is a locally-declared slice/array/Vec of a
    // fixed-width integer (`buf: &[u8; N]`): the element's type is the cast's
    // source, so `buf[0] as u32` is a provable widening.
    let element_type = cast_operand_indexed_element_type(node, source)?;
    numeric_type(&element_type)
}

/// A `float_literal` operand (`1.0 as f32`) is exempt unconditionally: a written
/// float constant is the programmer's chosen representation, with no fallible
/// `as` alternative. A `unary_expression` operand — a deref (`*x as i8`) or a
/// negation — is ceded to `rust-no-lossy-as-cast`, which owns those spans.
///
/// Plain `integer_literal` operands are NOT exempted here: they are range-checked
/// by `cast_operand_literal_value` + `literal_fits`, so an in-range literal
/// (`65 as i8`) is silenced while an out-of-range one (`200 as i8`) stays flagged.
fn is_literal_cast(node: tree_sitter::Node, _source: &[u8]) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|value| matches!(value.kind(), "float_literal" | "unary_expression"))
}

/// True if the integer `value` (parsed from a literal operand) lies within the
/// inclusive `[MIN, MAX]` range of the integer `target`, making the cast
/// lossless. Float targets never fit an integer literal here. Callers must have
/// already excluded `usize`/`isize` targets, whose host-dependent width is not
/// modelled (both cast rules filter those before reaching this check).
fn literal_fits(value: i128, target: NumericType) -> bool {
    let Some((min, max)) = target_int_bounds(target) else {
        return false;
    };
    value >= min && value <= max
}

/// The inclusive `[MIN, MAX]` bounds of an integer `target` as `i128`, or `None`
/// for a float target or a width too wide to represent in `i128` (`u128`/`i128`
/// MAX exceed `i128::MAX`, so a literal large enough to overflow them never
/// fits anyway). Shifts are checked to avoid overflow on the 128-bit boundary.
fn target_int_bounds(target: NumericType) -> Option<(i128, i128)> {
    match target.kind {
        NumericKind::Float => None,
        NumericKind::Unsigned => {
            // `u128::MAX` exceeds `i128::MAX`; cap at `i128::MAX` since no
            // `i128` literal value can be larger anyway.
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

    /// Run on a file next to the given `Cargo.toml` so the manifest
    /// (`proc-macro = true` exemption) resolves via `nearest_cargo_manifest`.
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_cargo(
            &Check,
            cargo_toml_contents,
            source,
            "src/x.rs",
        )
    }

    const PROC_MACRO_CARGO_TOML: &str = r#"
[package]
name = "derive-impl-like"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true
"#;

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "normal-lib"
version = "0.1.0"
edition = "2021"

[lib]
name = "normal_lib"
"#;

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
    fn allows_modulo_bounded_unsigned_dividend() {
        // Issue #6151: `(x % N) as uT` with a non-negative `x` and `N - 1 <=
        // uT::MAX` is always in range — the remainder of an unsigned dividend.
        assert!(run_on("fn f(width: u32) -> u8 { (width % 256) as u8 }").is_empty());
        assert!(run_on("fn f(n: u128) -> u32 { (n % 1_000_000) as u32 }").is_empty());
        // A same-file `let` with an unsigned annotation resolves the dividend too.
        assert!(run_on("fn f() -> u8 { let width: usize = 1000; (width % 256) as u8 }").is_empty());
        // A literal dividend is non-negative by construction.
        assert!(run_on("fn f() -> u8 { (100 % 256) as u8 }").is_empty());
        // A nested `%` is non-negative when its own dividend is.
        assert!(run_on("fn f(width: u32) -> u8 { ((width % 1000) % 256) as u8 }").is_empty());
        // A `& mask` is non-negative regardless of the masked value's sign — a
        // bitwise AND with a non-negative literal clears the sign bit.
        assert!(run_on("fn f(x: i32) -> u8 { ((x & 0xFF) % 16) as u8 }").is_empty());
    }

    #[test]
    fn flags_modulo_bound_exceeding_target() {
        // `300 - 1 = 299` does not fit `u8`: the remainder can be `256..=299`.
        assert_eq!(run_on("fn f(width: u32) -> u8 { (width % 300) as u8 }").len(), 1);
    }

    #[test]
    fn flags_modulo_signed_target() {
        // A signed target is never exempt — the remainder's range is irrelevant
        // to whether so large a value fits a signed type.
        assert_eq!(run_on("fn f(width: u32) -> i8 { (width % 256) as i8 }").len(), 1);
    }

    #[test]
    fn flags_modulo_signed_dividend() {
        // `s: i32` can be negative, so `s % 256` can be negative and `as u8`
        // wraps — the dividend is not proven non-negative.
        assert_eq!(run_on("fn f(s: i32) -> u8 { (s % 256) as u8 }").len(), 1);
    }

    #[test]
    fn flags_modulo_unresolved_dividend() {
        // A method return whose type lives in std (`Duration::as_nanos`) is not
        // provably unsigned from the AST, so the cast stays flagged.
        assert_eq!(
            run_on("fn f(d: Duration) -> u32 { (d.as_nanos() % 1_000_000) as u32 }").len(),
            1
        );
    }

    #[test]
    fn allows_min_clamped_narrowing() {
        // Issue #6174: `.min(BOUND) as uT` where the explicit clamp proves the
        // value fits. An unsigned-typed bound (`u64::MAX as u128`) forces the
        // receiver to share that unsigned type, so the clamped value is in range.
        assert!(
            run_on("fn f(d: Duration) -> u64 { d.as_nanos().min(u64::MAX as u128) as u64 }")
                .is_empty()
        );
        assert!(run_on("fn f(x: u64) -> u64 { x.min(u64::MAX) as u64 }").is_empty());
        // Narrowing target with an unsigned-cast literal bound (`200 as u64`).
        assert!(run_on("fn f(x: u64) -> u8 { x.min(200 as u64) as u8 }").is_empty());
        // Bare-literal bound with a provably non-negative (unsigned) receiver.
        assert!(run_on("fn f(v: u32) -> u8 { v.min(255) as u8 }").is_empty());
    }

    #[test]
    fn flags_min_clamp_unprovable_or_out_of_range() {
        // A signed receiver with a bare literal: `.min()` clamps only the upper
        // side, so `(-1i64).min(255) as u8` wraps to 255 — still lossy.
        assert_eq!(run_on("fn f(x: i64) -> u8 { x.min(255) as u8 }").len(), 1);
        // The clamp bound exceeds the target's range.
        assert_eq!(run_on("fn f(x: u128) -> u8 { x.min(u64::MAX as u128) as u8 }").len(), 1);
        // `.max()` bounds from below, not above.
        assert_eq!(run_on("fn f(x: u128) -> u64 { x.max(u64::MAX as u128) as u64 }").len(), 1);
    }

    #[test]
    fn allows_relocation_sized_pointer_write() {
        // Issue #5677: relocation-patch writes where the value cast's width
        // matches the destination pointer's pointee width are deliberate
        // truncation-to-store-width, not accidental loss.
        assert!(
            run_on("fn f() { unsafe { write_unaligned(addr as *mut u8, abs as u8); } }").is_empty()
        );
        assert!(
            run_on("fn f() { unsafe { write_unaligned(addr as *mut u16, abs as u16); } }")
                .is_empty()
        );
        assert!(
            run_on("fn f() { unsafe { write_unaligned(addr as *mut u32, abs as u32); } }")
                .is_empty()
        );
    }

    #[test]
    fn flags_pointer_write_with_width_mismatch() {
        // The value cast's width differs from the pointee width: still lossy.
        assert_eq!(
            run_on("fn f() { unsafe { write_unaligned(addr as *mut u8, abs as u16); } }").len(),
            1
        );
    }

    #[test]
    fn allows_same_width_signed_to_unsigned() {
        // `i32 as u32` is a same-width bit reinterpretation (#5972): every bit is
        // preserved and `as` is the only conversion that performs it, so the rule
        // does not flag it. A different-width sign change (`i32 as u16`) stays
        // flagged (see `test_flags_dangerous_narrowing_i32_to_u16`).
        assert!(run_on("fn f(x: i32) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn allows_raw_pointer_to_integer() {
        // Issue #5885: embedded DMA/MMIO address passing. A raw-pointer-to-integer
        // cast has no `From`/`TryFrom` path, so neither rule should flag it.
        // Inner `as *const _` / `as *mut <int>` cast operand.
        assert!(run_on("fn f() { let _ = executor as *const _ as u32; }").is_empty());
        assert!(
            run_on("fn f() { let _ = &mut self.table as *mut _ as *mut u32 as u32; }").is_empty()
        );
        // `.as_ptr()` / `.as_mut_ptr()` method-call operand.
        assert!(run_on("fn f() { let _ = task.as_ptr() as u32; }").is_empty());
        assert!(run_on("fn f() { let _ = regs.ch().cc().as_ptr() as u32; }").is_empty());
        assert!(run_on("fn f() { let _ = region.as_mut_ptr() as usize; }").is_empty());
        // `ptr::null()` / `null_mut()` operand, including turbofish.
        assert!(run_on("fn f() { let _ = core::ptr::null::<u8>() as u32; }").is_empty());
        assert!(run_on("fn f() { let _ = ptr::null_mut() as u32; }").is_empty());
    }

    #[test]
    fn flags_numeric_truncation_not_pointer() {
        // A genuine numeric narrowing must still fire — the pointer exemption is
        // shape-specific, not a blanket as-<int> exemption.
        assert_eq!(run_on("fn f(len: u64) -> u32 { len as u32 }").len(), 1);
        assert_eq!(run_on("fn f(x: u32) -> u8 { x as u8 }").len(), 1);
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
    fn allows_signed_to_unsigned_in_const_initializer() {
        // wasmerio/wasmer constants.rs: `as` is the only const-callable
        // conversion for a signed→unsigned bit-pattern embedding (#5679).
        assert!(run_on("const LEF32_GEQ_I32_MIN: u64 = i32::MIN as u64;").is_empty());
        assert!(run_on("const GEF64_LEQ_I32_MAX: u64 = i32::MAX as u64;").is_empty());
        assert!(run_on("const LEF32_GEQ_I64_MIN: u64 = i64::MIN as u64;").is_empty());
    }

    #[test]
    fn allows_cast_in_static_initializer() {
        assert!(run_on("static S: u64 = i64::MIN as u64;").is_empty());
    }

    #[test]
    fn allows_cast_in_const_fn_body() {
        assert!(run_on("const fn f(x: i64) -> u32 { x as u32 }").is_empty());
    }

    #[test]
    fn flags_cast_in_non_const_fn_body() {
        // A runtime body is unaffected — `try_into()` is available there.
        assert_eq!(run_on("fn f(x: i64) -> u32 { x as u32 }").len(), 1);
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
    fn repro_5033_byte_extraction_shift_not_flagged() {
        // `(bits >> 32) as u8` — high-bits-cleared byte extraction (HPACK
        // Huffman encoder pattern); the shift makes the truncation intentional.
        assert!(run_on("fn f(bits: u64) -> u8 { (bits >> 32) as u8 }").is_empty());
        assert!(run_on("fn f(x: u32) -> u8 { (x >> 24) as u8 }").is_empty());
    }

    #[test]
    fn repro_1289_plain_narrowing_still_flagged() {
        // No bitwise context — an arbitrary count/length narrowing stays flagged.
        assert_eq!(run_on("fn f(n: u32) -> u8 { n as u8 }").len(), 1);
    }

    #[test]
    fn repro_1309_len_as_u32_not_flagged() {
        // A collection's `.len()` is bounded by `isize::MAX`; `try_into` there
        // forces a semantically-impossible error path.
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
    fn repro_1309_len_as_f64_not_flagged() {
        // `len()` is unresolved by the AST, but its bounded shape makes the
        // cast safe regardless of the (float) target.
        assert!(run_on("fn f(v: V) -> f64 { v.len() as f64 }").is_empty());
    }

    #[test]
    fn repro_1309_unbounded_method_call_still_flagged() {
        // `.parse_count()` is not a size accessor — keep flagging it so the
        // exemption does not blanket-allow every method-call operand.
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
    fn repro_6105_method_call_as_u128_not_flagged() {
        // bevy_rapier: storing a `u64` Entity handle into a `u128` user_data
        // field. `to_bits()` is unresolved by the AST, but `u128` is the widest
        // integer type, so `<int> as u128` cannot overflow.
        assert!(run_on("fn f(e: Entity) -> u128 { e.to_bits() as u128 }").is_empty());
    }

    #[test]
    fn repro_6105_widening_int_idents_as_u128_not_flagged() {
        // Every unsigned/signed source widens losslessly into `u128`.
        for ty in ["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64"] {
            let src = format!("fn f(x: {ty}) -> u128 {{ x as u128 }}");
            assert!(run_on(&src).is_empty(), "{ty} as u128 should not be flagged");
        }
    }

    #[test]
    fn repro_6105_field_access_as_u128_not_flagged() {
        assert!(run_on("fn f(s: S) -> u128 { s.id as u128 }").is_empty());
    }

    #[test]
    fn repro_6105_float_as_u128_still_flagged() {
        // A float source truncates the fractional part — genuinely lossy.
        assert_eq!(run_on("fn f(x: f64) -> u128 { x as u128 }").len(), 1);
    }

    #[test]
    fn repro_6105_narrowing_from_u128_still_flagged() {
        // The widest-target exemption is one-directional: narrowing out of
        // `u128` stays flagged.
        assert_eq!(run_on("fn f(x: u128) -> u64 { x as u64 }").len(), 1);
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
    fn repro_6090_inferred_bool_local_as_u32_not_flagged() {
        // rapier generic_joint_constraint_builder.rs: `min_enabled` is bound by an
        // inferred-bool comparison `let min_enabled = lo <= dist;`, so
        // `min_enabled as u32` (the integer step of the bool→float mask) is a
        // lossless `bool as u32`, not a narrowing.
        let src = "fn f(lo: f64, dist: f64) -> u32 { \
                   let min_enabled = lo <= dist; min_enabled as u32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6090_inferred_non_bool_local_as_u32_still_flagged() {
        // Negative-space guard: `n` is bound to an arithmetic (non-bool)
        // expression, so `n as u32` is a genuine narrowing and stays flagged.
        let src = "fn f(a: u64, b: u64) -> u32 { let n = a + b; n as u32 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6090_non_bool_shadow_of_bool_local_still_flagged() {
        // Negative-space guard: the nearest binding wins. A later non-bool `n`
        // shadows an earlier bool `n`, so `n as u32` is a real narrowing.
        let src = "fn f(a: u64, b: u64) -> u32 { let n = a < b; let n = a + b; n as u32 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6090_bool_binding_in_sibling_block_does_not_leak() {
        // Negative-space guard: a bool binding inside a sibling inner block does
        // not enclose the cast, so the outer `n` (a non-bool param) governs and
        // the narrowing stays flagged.
        let src = "fn f(n: u64) -> u32 { { let n = n < 1; let _ = n; } n as u32 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5886_method_returning_bool_as_u8_not_flagged() {
        // Issue #5886 (rp2040-hal rosc.rs): `self.get_random_bit() as u8` where
        // `get_random_bit(&self) -> bool`. The same-file signature resolves the
        // method to `-> bool`, so `bool as u8` is lossless.
        let src = "fn get_random_bit(&self) -> bool { true } \
                   fn fill(&self) -> u8 { self.get_random_bit() as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5886_free_fn_returning_bool_as_u8_not_flagged() {
        let src = "fn b() -> bool { true } fn f() -> u8 { b() as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5886_method_returning_numeric_still_flagged() {
        // A same-file callee returning a non-bool numeric is a genuine narrowing:
        // `self.tally() as u8` where `tally(&self) -> u32` stays flagged.
        let src = "fn tally(&self) -> u32 { 0 } fn f(&self) -> u8 { self.tally() as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5886_unresolvable_method_still_flagged() {
        // The callee is not defined in the file, so its return type is unknown —
        // the narrowing stays a finding.
        assert_eq!(run_on("fn f(s: S) -> u8 { s.unknown() as u8 }").len(), 1);
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
    fn repro_3859_cast_in_impl_method_still_flagged() {
        // A cast inside an `impl Enum` method is a runtime body, not a
        // discriminant — it must keep flagging.
        let src = "enum E { A } impl E { fn f(&self, x: u32) -> i8 { x as i8 } }";
        assert_eq!(run_on(src).len(), 1);
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
    fn repro_3832_self_as_u8_in_impl_data_enum_still_flagged() {
        // A data-carrying enum has no discriminant `as`-cast semantics, so the
        // exemption must not apply.
        let src = "enum E { A(u32), B } impl E { fn bit(self) -> u8 { self as u8 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_4811_external_enum_variant_path_as_i32_not_flagged() {
        // `lsp_server::ErrorCode::InvalidParams as i32` reads an imported
        // fieldless enum's discriminant; `as` is the only conversion that
        // compiles (no `From`/`TryFrom<ErrorCode> for i32`). The enum is not in
        // this file, so the cast is exempted by the `<Type>::<Variant>` shape.
        let src = "fn f() -> i32 { lsp_server::ErrorCode::InvalidParams as i32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_4811_external_enum_two_segment_path_as_u8_not_flagged() {
        let src = "fn f() -> u8 { Direction::North as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_4811_external_const_path_as_u8_still_flagged() {
        // `mod::MAX_LEN` is a SCREAMING_SNAKE_CASE const, not an enum variant —
        // keep flagging so the exemption stays scoped to discriminant reads.
        let src = "fn f() -> u8 { limits::MAX_LEN as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_4811_external_lowercase_path_as_u8_still_flagged() {
        // A lowercase final segment is a function/module item, not a variant.
        let src = "fn f() -> u8 { config::default_size as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_4811_in_file_data_enum_variant_path_still_flagged() {
        // The enum IS in this file and is data-carrying — the shape heuristic
        // must not override the in-file truth.
        let src = "enum E { A(u32), B } fn f() -> u8 { E::B as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_4804_char_literal_as_u32_not_flagged() {
        // `'=' as u32` reads a char's code point; a `char` is ≤ 0x10FFFF (21
        // bits), so the cast to u32 is always lossless.
        assert!(run_on("const CHAR_ASSIGN: u32 = '=' as u32;").is_empty());
    }

    #[test]
    fn repro_4804_char_binding_as_u32_not_flagged() {
        assert!(run_on("fn f(c: char) -> u32 { c as u32 }").is_empty());
    }

    #[test]
    fn repro_4804_char_literal_as_u8_still_flagged() {
        // u8 is only 8 bits — a `char` can exceed it, so the cast can truncate.
        assert_eq!(run_on("fn f() -> u8 { 'a' as u8 }").len(), 1);
    }

    #[test]
    fn repro_4804_char_literal_as_i32_not_flagged() {
        // i32 holds the full 21-bit code-point range losslessly.
        assert!(run_on("fn f() -> i32 { 'a' as i32 }").is_empty());
    }

    #[test]
    fn repro_5165_if_let_some_char_from_u32_as_u32_not_flagged() {
        // `if let Some(ch) = char::from_u32(..)` binds `ch: char`; `char as u32`
        // is lossless (≤ 0x10FFFF fits u32). The source is not numeric.
        let src = "fn f(x: u32) -> u32 { \
                   if let Some(ch) = char::from_u32(x) { ch as u32 } else { 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5165_if_let_some_char_core_path_as_u32_not_flagged() {
        // The issue's exact path shape: `::core::char::from_u32`.
        let src = "fn f(x: u32) -> u32 { \
                   if let Some(ch) = ::core::char::from_u32(x) { ch as u32 } else { 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5165_if_let_some_char_from_digit_as_u32_not_flagged() {
        let src = "fn f(d: u32) -> u32 { \
                   if let Some(ch) = char::from_digit(d, 10) { ch as u32 } else { 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5165_if_let_some_non_char_from_u32_still_flagged() {
        // `Foo::from_u32` is not a char-returning function — the unwrapped
        // binding is not a char, so a narrowing numeric cast stays flagged.
        let src = "fn f(x: u64) -> u8 { \
                   if let Some(n) = Foo::from_u32(x) { n as u8 } else { 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5165_if_let_some_char_as_u8_still_flagged() {
        // u8 is only 8 bits — even a char-bound operand truncates, so the
        // narrowing cast stays flagged (the char exemption needs ≥ 21 bits).
        let src = "fn f(x: u32) -> u8 { \
                   if let Some(ch) = char::from_u32(x) { ch as u8 } else { 0 } }";
        assert_eq!(run_on(src).len(), 1);
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
        // `val <= u8::MAX` proves the value fits u8.
        assert!(
            run_on("fn w(val: u64) -> u8 { if val <= 255 { val as u8 } else { 0 } }").is_empty()
        );
    }

    #[test]
    fn repro_4922_unguarded_narrowing_still_flagged() {
        // No range guard — the narrowing stays a real finding.
        assert_eq!(run_on("fn f(n: u64) -> u8 { n as u8 }").len(), 1);
    }

    #[test]
    fn repro_4922_loose_guard_still_flagged() {
        // The bound exceeds u8's range, so the value can still overflow.
        let src = "fn w(val: u64) -> u8 { if val < 1000 { val as u8 } else { 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_4922_signed_source_still_flagged() {
        // A signed source can be negative; an upper-bound guard alone does not
        // prove it fits the unsigned target.
        let src = "fn w(val: i64) -> u8 { if val < 256 { val as u8 } else { 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6173_else_branch_upper_bound_not_flagged() {
        // The tonic xds pattern: `if value > 100 { Err } else { value as u8 }`.
        // The else is reached only when `value <= 100`, which fits u8.
        let src =
            "fn new(value: u32) -> Result<u8, ()> { if value > 100 { Err(()) } else { Ok(value as u8) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6173_early_exit_guard_not_flagged() {
        // A preceding early-exit guard whose `then` branch diverges proves the
        // value fits before the fallthrough cast is reached.
        let src = "fn g(value: u32) -> u8 { if value > 100 { return 0; } value as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6173_non_diverging_guard_still_flagged() {
        // The `then` branch does not diverge, so a large value flows through to the
        // cast — still a real finding.
        let src = "fn g(value: u32) -> u8 { if value > 100 { let _x = 1; } value as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_assert_max_bound_not_flagged() {
        // The issue's shape (hyper encode.rs): an `assert!(x <= u8::MAX as u64)`
        // on a preceding line proves the value fits the target.
        let src = "fn f(x: u64) -> u8 { assert!(x <= u8::MAX as u64); let y = x as u8; y }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5034_assert_literal_bound_not_flagged() {
        // A numeric-literal upper bound within the target's range.
        let src = "fn g(n: u64) -> u8 { assert!(n < 256); n as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5034_debug_assert_inclusive_bound_not_flagged() {
        let src = "fn h(n: u64) -> u8 { debug_assert!(n <= 255); n as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5034_assert_reversed_bound_not_flagged() {
        // The symmetric `BOUND >= name` form.
        let src = "fn f(n: u64) -> u8 { assert!(255 >= n); n as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5034_no_assert_still_flagged() {
        let src = "fn f(n: u64) -> u8 { n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_assert_on_different_variable_still_flagged() {
        // The assert bounds `m`, not the cast operand `n`.
        let src = "fn f(n: u64, m: u64) -> u8 { assert!(m <= 255); n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_assert_without_upper_bound_still_flagged() {
        // A non-comparison assert proves nothing about the cast operand.
        let src = "fn f(n: u64) -> u8 { assert!(n > 0); n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_assert_loose_bound_still_flagged() {
        // The asserted bound exceeds u8's range, so the value can still overflow.
        let src = "fn f(n: u64) -> u8 { assert!(n < 1000); n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_assert_method_bound_still_flagged() {
        // `self.remaining()` is not a provable numeric bound — without resolving
        // it, the cast cannot be proven safe.
        let src = "fn f(&self, n: u64) -> u8 { assert!(n <= self.remaining()); n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_reassigned_after_assert_still_flagged() {
        // `n` is overwritten after the assert, breaking the bound.
        let src = "fn f(mut n: u64, m: u64) -> u8 { assert!(n <= 255); n = m; n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_signed_source_still_flagged() {
        // A signed source can be negative; an upper-bound assert alone does not
        // prove it fits the unsigned target.
        let src = "fn f(n: i64) -> u8 { assert!(n <= 255); n as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5034_assert_with_message_not_flagged() {
        // A trailing message argument (the idiomatic `assert!(cond, "msg")` form)
        // does not break bound detection: the first comparison still proves it.
        let src = "fn f(n: u64) -> u8 { assert!(n <= 255, \"too big\"); n as u8 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5257_read_bits_within_target_not_flagged() {
        // A bit-reader reading N <= target bits yields a value that fits the
        // target losslessly — the codec/bitstream parsing idiom.
        assert!(run_on("fn f(r: R) -> u8 { r.read_bits(4) as u8 }").is_empty());
        assert!(run_on("fn f(r: R) -> u8 { r.read_bits(8) as u8 }").is_empty());
        assert!(run_on("fn f(bs: B) -> u16 { bs.read_bits_leq32(16)? as u16 }").is_empty());
        assert!(run_on("fn f(r: R) -> u8 { r.get_bits(2)? as u8 }").is_empty());
    }

    #[test]
    fn repro_5257_read_bits_over_target_still_flagged() {
        // Reading more bits than the target holds is a genuine narrowing.
        assert_eq!(run_on("fn f(r: R) -> u8 { r.read_bits(9) as u8 }").len(), 1);
    }

    #[test]
    fn repro_5257_read_bits_signed_target_reserves_sign_bit() {
        // `i8` holds 7 value bits; an 8-bit read does not fit losslessly.
        assert_eq!(run_on("fn f(r: R) -> i8 { r.read_bits(8) as i8 }").len(), 1);
        // 7 bits fit a signed `i8`.
        assert!(run_on("fn f(r: R) -> i8 { r.read_bits(7) as i8 }").is_empty());
    }

    #[test]
    fn repro_5257_read_bits_non_literal_count_still_flagged() {
        // A non-literal count is not statically bounded.
        assert_eq!(run_on("fn f(r: R, n: u32) -> u8 { r.read_bits(n) as u8 }").len(), 1);
    }

    #[test]
    fn repro_5257_plain_numeric_cast_still_flagged() {
        // A real numeric narrowing with no bit-reader operand stays flagged.
        assert_eq!(run_on("fn f(x: u32) -> u8 { x as u8 }").len(), 1);
    }

    #[test]
    fn repro_6092_leading_zeros_as_smaller_int_not_flagged() {
        // Issue #6092 (dimforge/parry z_order.rs): `leading_zeros()` returns a
        // `u32` bounded by the receiver's bit-width (≤ 128), so casting it to a
        // smaller integer that holds 128 is provably lossless.
        assert!(run_on("fn f(x: u64) -> i16 { 64i16 - (x ^ x).leading_zeros() as i16 }").is_empty());
        assert!(run_on("fn f(x: u32) -> u8 { x.leading_zeros() as u8 }").is_empty());
        assert!(run_on("fn f(n: u64) -> u16 { n.trailing_zeros() as u16 }").is_empty());
        assert!(run_on("fn f(v: u128) -> u8 { v.count_ones() as u8 }").is_empty());
        assert!(run_on("fn f(v: u64) -> u8 { v.count_zeros() as u8 }").is_empty());
        assert!(run_on("fn f(v: u32) -> i32 { v.leading_ones() as i32 }").is_empty());
        assert!(run_on("fn f(v: u32) -> u16 { v.trailing_ones() as u16 }").is_empty());
    }

    #[test]
    fn repro_6092_bit_count_as_i8_still_flagged() {
        // `i8` max is 127 < 128 — the maximum bit-count result (128) does not fit,
        // so the carve-out must not apply.
        assert_eq!(run_on("fn f(x: u128) -> i8 { x.leading_zeros() as i8 }").len(), 1);
    }

    #[test]
    fn repro_6092_non_bit_count_method_still_flagged() {
        // A non-bit-count no-arg method is unbounded — keep flagging it so the
        // exemption stays scoped to the bit-count method set.
        assert_eq!(run_on("fn f(x: V) -> u8 { x.some_other_method() as u8 }").len(), 1);
    }

    #[test]
    fn repro_6092_arithmetic_on_bit_count_result_still_flagged() {
        // `(x.leading_zeros() + offset) as u8` — the operand is a binary
        // expression, not a bare bit-count call, so the bound no longer holds.
        let src = "fn f(x: u64, offset: u32) -> u8 { (x.leading_zeros() + offset) as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5260_repr_enum_field_as_u8_not_flagged() {
        // `self.dispose_op as u8` where the field is a `#[repr(u8)]` enum is
        // lossless; this rule must not flag it either.
        let src = "#[repr(u8)] enum DisposeOp { None = 0, Background = 1 } \
                   struct Frame { dispose_op: DisposeOp } \
                   impl Frame { fn ser(&self) -> u8 { self.dispose_op as u8 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5260_integer_field_narrowing_still_flagged() {
        // The repr-enum exemption must not leak to a genuine integer narrowing:
        // a `u16` field `as u8` is lossy and still flagged here.
        let src = "struct S { count: u16 } fn g(s: &S) -> u8 { s.count as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5260_repr_u16_enum_field_as_u8_still_flagged() {
        // A `#[repr(u16)]` enum's discriminant does not fit u8; the exemption
        // must not apply, so the narrowing stays a finding.
        let src = "#[repr(u16)] enum E { A } \
                   struct S { f: E } \
                   fn g(s: &S) -> u8 { s.f as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5209_byte_literal_as_i8_not_flagged() {
        // The issue's exact shape (console-rs WinAPI `CHAR`): `b' ' as i8`. The
        // byte value 32 fits i8's -128..=127, so the cast is provably lossless.
        assert!(run_on("fn f() -> i8 { b' ' as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { b'A' as i8 }").is_empty());
    }

    #[test]
    fn repro_5209_hex_and_decimal_literal_in_range_not_flagged() {
        assert!(run_on("fn f() -> i8 { 0x41 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 65 as i8 }").is_empty());
    }

    #[test]
    fn repro_5209_negative_literal_in_range_not_flagged() {
        assert!(run_on("fn f() -> i8 { -5 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { -128 as i8 }").is_empty());
    }

    #[test]
    fn repro_5209_octal_binary_and_suffixed_literal_in_range_not_flagged() {
        assert!(run_on("fn f() -> i8 { 0o17 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 0b0101 as i8 }").is_empty());
        assert!(run_on("fn f() -> i8 { 65u8 as i8 }").is_empty());
        assert!(run_on("fn f() -> u16 { 1_000 as u16 }").is_empty());
    }

    #[test]
    fn repro_5209_out_of_range_literal_still_flagged() {
        // 200 > i8::MAX (127): a genuine lossy literal cast stays flagged.
        assert_eq!(run_on("fn f() -> i8 { 200 as i8 }").len(), 1);
        // 0xFF = 255 > 127.
        assert_eq!(run_on("fn f() -> i8 { 0xFF as i8 }").len(), 1);
        // 300 > u8::MAX (255).
        assert_eq!(run_on("fn f() -> u8 { 300 as u8 }").len(), 1);
    }

    #[test]
    fn repro_5209_non_literal_operand_still_flagged() {
        // A variable operand is not a literal — its value is unknown, so a
        // narrowing cast stays flagged.
        assert_eq!(run_on("fn f(x: i32) -> i8 { x as i8 }").len(), 1);
    }

    #[test]
    fn repro_5262_match_guard_not_negative_not_flagged() {
        // The Symphonia pattern: `Some(diff) if !diff.is_negative()` proves the
        // i64 binding is non-negative, so `diff as u64` is lossless. The source
        // type is unresolved (a match-arm binding has no AST annotation), so the
        // 64-bit-or-wider target gates the exemption.
        let src = "fn f(o: Option<i64>) -> Option<u64> { \
                   match o { Some(diff) if !diff.is_negative() => Some(diff as u64), \
                   _ => None } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5262_if_ge_zero_not_flagged() {
        // `if x >= 0` proves the signed source is non-negative; `i32 as u32` is a
        // signed→unsigned widening (equal width here) that is then lossless.
        assert!(run_on("fn f(x: i32) -> u32 { if x >= 0 { x as u32 } else { 0 } }").is_empty());
    }

    #[test]
    fn repro_5262_is_positive_guard_not_flagged() {
        assert!(
            run_on("fn f(x: i64) -> u64 { if x.is_positive() { x as u64 } else { 0 } }").is_empty()
        );
    }

    #[test]
    fn repro_5262_unresolved_unguarded_signed_to_unsigned_still_flagged() {
        // A same-width signed→unsigned cast whose source resolves is now exempt as
        // a bit reinterpretation (#5972); the guard path still governs an
        // *unresolved* source — a match-arm binding with no annotation and no
        // non-negativity proof stays flagged.
        let src = "fn f(o: Option<i64>) -> Option<u64> { \
                   match o { Some(diff) => Some(diff as u64), _ => None } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5262_guarded_narrowing_still_flagged() {
        // `i64 >= 0` does not prove the value fits u8 — a non-negative i64 can
        // exceed 255, so the narrowing stays flagged.
        let src = "fn f(x: i64) -> u8 { if x >= 0 { x as u8 } else { 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5318_byte_array_index_as_u32_not_flagged() {
        // The flate2 gz footer pattern: `buf: &[u8; 8]`, so `buf[N]` is `u8` and
        // `buf[N] as u32` is a provable widening (u8 -> u32, always lossless).
        let src = "fn finish(buf: &[u8; 8]) -> u32 { \
                   (buf[0] as u32) | ((buf[1] as u32) << 8) \
                   | ((buf[2] as u32) << 16) | ((buf[3] as u32) << 24) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5318_byte_slice_index_as_u32_not_flagged() {
        assert!(run_on("fn f(buf: &[u8]) -> u32 { buf[0] as u32 }").is_empty());
    }

    #[test]
    fn repro_5318_vec_u8_index_as_u32_not_flagged() {
        assert!(run_on("fn f(buf: Vec<u8>) -> u32 { buf[0] as u32 }").is_empty());
    }

    #[test]
    fn repro_5318_box_slice_u8_index_as_u32_not_flagged() {
        assert!(run_on("fn f(buf: Box<[u8]>) -> u32 { buf[0] as u32 }").is_empty());
    }

    #[test]
    fn repro_5318_u16_element_as_u32_not_flagged() {
        // u16 -> u32 is still widening.
        assert!(run_on("fn f(buf: &[u16]) -> u32 { buf[0] as u32 }").is_empty());
    }

    #[test]
    fn repro_5318_narrowing_index_cast_still_flagged() {
        // The element type is u32 and the target u8 — a genuine narrowing.
        assert_eq!(run_on("fn f(buf: &[u32]) -> u8 { buf[0] as u8 }").len(), 1);
    }

    #[test]
    fn repro_5318_unresolvable_base_index_cast_still_flagged() {
        // The base comes from a method return — its element type is not locally
        // resolvable, so the cast stays flagged (no guessing).
        assert_eq!(run_on("fn f(thing: T) -> u32 { thing.bytes()[0] as u32 }").len(), 1);
    }

    #[test]
    fn repro_5318_untyped_base_index_cast_still_flagged() {
        // `let buf = make();` has no annotation — element type unresolvable.
        let src = "fn f() -> u32 { let buf = make(); buf[0] as u32 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5357_usize_as_u64_not_flagged() {
        // The async-std io::copy pattern: a byte count accumulated into a u64
        // total. `u64::from(usize)` is not in stable Rust, so `as` is the only
        // infallible conversion.
        assert!(run_on("fn f(x: usize) -> u64 { x as u64 }").is_empty());
    }

    #[test]
    fn repro_5357_usize_as_u128_not_flagged() {
        assert!(run_on("fn f(x: usize) -> u128 { x as u128 }").is_empty());
    }

    #[test]
    fn repro_5357_isize_as_i64_not_flagged() {
        assert!(run_on("fn f(x: isize) -> i64 { x as i64 }").is_empty());
    }

    #[test]
    fn repro_5357_isize_as_i128_not_flagged() {
        assert!(run_on("fn f(x: isize) -> i128 { x as i128 }").is_empty());
    }

    #[test]
    fn repro_5357_usize_as_u32_still_flagged() {
        // Narrowing on a 64-bit target: `usize` can exceed `u32`, so the cast
        // is lossy and must keep recommending `try_from`.
        assert_eq!(run_on("fn f(x: usize) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn repro_5357_usize_as_i64_sign_change_still_flagged() {
        // A sign change is not a same-signedness widening; a `usize` near
        // `u64::MAX` overflows `i64`, so the cast stays flagged.
        assert_eq!(run_on("fn f(x: usize) -> i64 { x as i64 }").len(), 1);
    }

    #[test]
    fn repro_5357_let_binding_usize_as_u64_not_flagged() {
        // The exemption resolves the source type from a `let` annotation too.
        let src = "fn f() -> u64 { let n: usize = g(); n as u64 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5122_is_ascii_then_some_char_as_u8_not_flagged() {
        // Issue #5122 (chumsky src/text.rs): `is_ascii()` proves `*self` is in
        // `0..=127`, so the `*self as u8` it gates cannot truncate.
        let src = "fn to_ascii(&self) -> Option<u8> { self.is_ascii().then_some(*self as u8) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5122_if_is_ascii_char_as_u8_not_flagged() {
        let src =
            "fn f(ch: char) -> Option<u8> { if ch.is_ascii() { Some(ch as u8) } else { None } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5122_unguarded_char_as_u8_still_flagged() {
        // No `is_ascii` guard: a `char` can exceed 127, so `as u8` truncates and
        // the cast must stay flagged.
        assert_eq!(run_on("fn f(ch: char) -> u8 { ch as u8 }").len(), 1);
    }

    #[test]
    fn repro_5122_is_ascii_guard_on_other_value_still_flagged() {
        // The guard tests a different value than the one cast, so it proves
        // nothing about the cast operand.
        let src = "fn f(a: char, b: char) -> Option<u8> { a.is_ascii().then_some(b as u8) }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5593_from_bits_arg_i32_as_u32_not_flagged() {
        // Issue #5593 (qdrant simple_avx.rs): `f32::from_bits(p1 as u32)` where
        // `p1` is the i32 the `_mm_extract_ps` intrinsic returns. The cast feeds
        // a bit-reinterpretation sink, so it is the correct same-width tool — a
        // `u32::try_from` would reject negative bit patterns.
        let src = "fn f() -> f32 { let p1 = _mm_extract_ps(h, 0); f32::from_bits(p1 as u32) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5593_f64_from_bits_arg_i64_as_u64_not_flagged() {
        let src = "fn f() -> f64 { let p = q(); f64::from_bits(p as u64) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5593_from_bits_through_parens_not_flagged() {
        // A single parenthesized wrapper between the cast and the argument list
        // is transparent.
        let src = "fn f() -> f32 { let p = q(); f32::from_bits((p as u32)) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5593_non_from_bits_call_arg_still_flagged() {
        // A genuinely-narrowing cast feeding an ordinary call is not a
        // bit-reinterpretation sink, so the `from_bits` anchor must not exempt it:
        // `i64 as u32` discards 32 bits. (A *same-width* `i32 as u32` is exempt as
        // a bit reinterpretation regardless of the sink — see #5972.)
        let src = "fn f(p: i64) -> u32 { consume(p as u32) }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5550_external_repr_enum_param_as_u32_not_flagged() {
        // The issue's exact shape (naga SPIR-V backend): a parameter typed as an
        // imported `#[repr(u32)]` enum cast to u32. `as` is the only conversion
        // the language offers (no `From<SourceLanguage> for u32`); the repr makes
        // it lossless. The type is not in this file, so the PascalCase type-name
        // shape identifies it as a fieldless-enum discriminant read.
        let src =
            "fn source(source_language: spirv::SourceLanguage) -> u32 { source_language as u32 }";
        assert!(run_on(src).is_empty());
        let src2 = "fn decorate(decoration: spirv::Decoration) -> u32 { decoration as u32 }";
        assert!(run_on(src2).is_empty());
    }

    #[test]
    fn repro_5550_local_repr_enum_binding_as_u32_not_flagged() {
        // A `let` binding annotated with an imported enum type, cast to u32.
        let src = "fn f() -> u32 { let op: spirv::Op = next(); op as u32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5550_bare_pascal_type_binding_still_flagged() {
        // A bare (unscoped) type name with no matching `enum_item` in the file is
        // indistinguishable from a local numeric type alias (`type Decoration =
        // u64`), so it stays conservatively flagged. (A bare name that DOES resolve
        // to an in-file fieldless enum is exempt — see the #6172 test below.)
        let src = "fn f(d: Decoration) -> u8 { d as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6172_from_fieldless_enum_for_int_not_flagged() {
        // Issue #6172 (hyperium/tonic): `code as i32` inside
        // `impl From<Code> for i32` where `Code` is a fieldless in-file enum reads
        // the discriminant — the only way to implement that `From` (`i32::from`
        // would recurse; no `TryFrom` exists). The bare-name operand resolves to an
        // in-file fieldless `enum_item`, so it is exempt.
        let src = "enum Code { Ok = 0, Cancelled = 1 } \
                   impl From<Code> for i32 { fn from(code: Code) -> i32 { code as i32 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6172_data_carrying_enum_param_still_flagged() {
        // A bare-name operand whose in-file enum carries data is not a plain
        // discriminant read, so the cast stays flagged.
        let src = "enum E { A(u32), B } fn f(e: E) -> u8 { e as u8 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_5550_numeric_param_narrowing_still_flagged() {
        // A genuinely numeric operand (resolved primitive) is NOT enum-shaped:
        // `u64 as u32` stays a lossy narrowing finding.
        assert_eq!(run_on("fn f(x: u64) -> u32 { x as u32 }").len(), 1);
    }

    #[test]
    fn repro_5600_u64_as_i64_feeding_simd_intrinsic_not_flagged() {
        // Issue #5600 (lance ex_dot.rs): `_mm_set_epi64x(hi as i64, lo as i64)`
        // where `hi`/`lo` are `u64`. The SSE2 intrinsic takes `i64` lanes (Intel
        // ABI); the `u64 as i64` is a same-width bit reinterpretation feeding it,
        // so it is the correct tool — a `try_from` would reject bit patterns
        // above `i64::MAX`.
        let src = "fn f(hi: u64, lo: u64) -> __m128i { _mm_set_epi64x(hi as i64, lo as i64) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5600_u64_literal_as_i64_feeding_simd_intrinsic_not_flagged() {
        // The hex bit-mask shape: `0x8040_2010_0804_0201u64 as i64` exceeds
        // `i64::MAX`, so the literal-range check does not exempt it; the SIMD
        // intrinsic argument anchor does.
        let src = "fn f() -> __m128i { _mm_set1_epi64x(0x8040_2010_0804_0201u64 as i64) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5600_call_operand_u64_as_i64_feeding_simd_intrinsic_not_flagged() {
        // The call-operand shape: `splat_byte(b1) as i64` where `splat_byte`
        // returns `u64` (unresolvable source). The same-width target gates the
        // SIMD anchor.
        let src = "fn f() -> __m128i { _mm_set_epi64x(splat_byte(b1) as i64, splat_byte(b0) as i64) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_5600_avx2_and_avx512_intrinsics_not_flagged() {
        // `_mm256_*` and `_mm512_*` lane setters take signed lanes too.
        assert!(run_on("fn f(x: u32) -> __m256i { _mm256_set1_epi32(x as i32) }").is_empty());
        assert!(run_on("fn f(x: u64) -> __m512i { _mm512_set1_epi64(x as i64) }").is_empty());
    }

    #[test]
    fn repro_5600_narrowing_cast_feeding_simd_intrinsic_still_flagged() {
        // A genuinely narrowing cast feeding a SIMD intrinsic is NOT same-width,
        // so the anchor must not exempt it: `u64 as i32` discards 32 bits.
        assert_eq!(run_on("fn f(x: u64) -> __m128i { _mm_set1_epi32(x as i32) }").len(), 1);
    }

    #[test]
    fn repro_5600_same_width_cast_outside_simd_intrinsic_still_flagged() {
        // The anchor is scoped to SIMD-intrinsic arguments: a same-width
        // signed↔unsigned cast feeding an ordinary call stays flagged when the
        // source is unresolved (no non-negativity proof).
        assert_eq!(run_on("fn f() -> u64 { consume(load() as u64) }").len(), 1);
    }

    #[test]
    fn repro_5844_usize_as_u32_in_proc_macro_crate_not_flagged() {
        // Issue #5844 (hecs/specs-derive): `i as u32` building a `syn::Index`
        // field index, where `i` is an `.enumerate()` counter over struct fields.
        // In a `proc-macro = true` crate the value is a compile-time field count
        // (trivially < u32::MAX), and `syn::Index.index` is a `u32`, so `as` is
        // the idiomatic constructor input.
        let src = "fn f(i: usize) -> Member { \
                   Member::Unnamed(Index { index: i as u32, span: s }) }";
        assert!(run_on_with_cargo(PROC_MACRO_CARGO_TOML, src).is_empty());
    }

    #[test]
    fn repro_5972_u8_as_i8_same_width_reinterpret_not_flagged() {
        // Issue #5972 (fancy-regex prev_codepoint_ix): `u8 as i8` is a same-width
        // two's-complement bit reinterpretation for UTF-8 continuation-byte
        // detection — no bits are lost, and `as` is the only conversion that
        // performs it (`i8::from(u8)` does not exist).
        assert!(run_on("fn f(b: u8) -> i8 { b as i8 }").is_empty());
        assert!(run_on("fn f(b: i8) -> u8 { b as u8 }").is_empty());
        assert!(run_on("fn f(n: u16) -> i16 { n as i16 }").is_empty());
        assert!(run_on("fn f(n: i32) -> u32 { n as u32 }").is_empty());
        assert!(run_on("fn f(n: u64) -> i64 { n as i64 }").is_empty());
    }

    #[test]
    fn repro_5972_as_bytes_index_as_i8_not_flagged() {
        // The issue's exact shape: `let bytes = s.as_bytes(); (bytes[ix] as i8)`.
        // `.as_bytes()` returns `&[u8]`, so the indexed element is `u8` and the
        // cast to `i8` is the same-width reinterpretation above.
        let src = "fn f(s: &str, ix: usize) -> bool { \
                   let bytes = s.as_bytes(); (bytes[ix] as i8) >= -0x40 }";
        assert!(run_on(src).is_empty());
        // Direct `.as_bytes()[ix]` base, without the intermediate binding.
        assert!(run_on("fn f(s: &str, ix: usize) -> i8 { s.as_bytes()[ix] as i8 }").is_empty());
    }

    #[test]
    fn repro_5972_cross_width_signed_unsigned_still_flagged() {
        // Different widths stay lossy — the same-width carve-out must not leak:
        // `u16 as u8` and `i32 as i16` discard bits, `u16 as i8` discards bits
        // and may wrap. (axum's `i32 as u32` etc. are same-width and now exempt.)
        assert_eq!(run_on("fn f(x: u16) -> u8 { x as u8 }").len(), 1);
        assert_eq!(run_on("fn f(x: u32) -> u16 { x as u16 }").len(), 1);
        assert_eq!(run_on("fn f(x: i32) -> i16 { x as i16 }").len(), 1);
        assert_eq!(run_on("fn f(x: u16) -> i8 { x as i8 }").len(), 1);
    }

    #[test]
    fn repro_5972_as_bytes_index_narrowing_still_flagged() {
        // The `.as_bytes()` resolution yields `u8`, so a narrowing to a smaller-
        // than-`u8` target is impossible; but a non-`as_bytes` method base stays
        // unresolved and a genuine narrowing keeps flagging.
        assert_eq!(run_on("fn f(s: &str, ix: usize) -> u8 { s.chunks()[ix] as u8 }").len(), 1);
    }

    #[test]
    fn repro_5844_usize_as_u32_in_normal_crate_still_flagged() {
        // The exemption is proc-macro-only: in a normal library crate a
        // `usize as u32` narrowing operates on runtime data and stays flagged.
        assert_eq!(
            run_on_with_cargo(LIB_CARGO_TOML, "fn f(i: usize) -> u32 { i as u32 }").len(),
            1
        );
    }

    #[test]
    fn repro_6150_saturating_wildcard_cast_not_flagged() {
        // The Floyd-Steinberg saturation idiom: preceding arms clamp every value
        // outside `[0, 0xFF]`, so the wildcard arm's `val as u8` is provably in
        // range and must not be flagged.
        let src = "fn f(x: i16) -> u8 { match x { \
                   val if val < 0 => 0, \
                   val if val > 0xFF => 0xFF, \
                   val => val as u8 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6150_image_rs_snippet_not_flagged() {
        // The issue's exact shape (image-rs/image colorops.rs `diffuse_err`).
        let src = "fn diffuse_err<P: Pixel<Subpixel = u8>>(pixel: &mut P, error: [i16; 3], factor: i16) { \
                   for (e, c) in error.iter().zip(pixel.channels_mut().iter_mut()) { \
                   *c = match <i16 as From<_>>::from(*c) + e * factor / 16 { \
                   val if val < 0 => 0, \
                   val if val > 0xFF => 0xFF, \
                   val => val as u8 } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6150_inclusive_mirrored_bounds_not_flagged() {
        // The `<= -1` floor and `>= N + 1` ceiling forms (decimal `256` == u8 max
        // plus one) prove the same range.
        let src = "fn f(x: i32) -> u8 { match x { \
                   val if val <= -1 => 0, \
                   val if val >= 256 => 255, \
                   val => val as u8 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn repro_6150_lower_bound_only_still_flagged() {
        // Only the floor arm is present; nothing bounds the value above 0xFF, so
        // `val as u8` can still overflow and stays flagged.
        let src = "fn f(x: i16) -> u8 { match x { \
                   val if val < 0 => 0, \
                   val => val as u8 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6150_upper_bound_only_still_flagged() {
        // Only the ceiling arm is present; nothing rules out a negative value
        // wrapping on the unsigned cast, so it stays flagged.
        let src = "fn f(x: i16) -> u8 { match x { \
                   val if val > 0xFF => 0xFF, \
                   val => val as u8 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6150_mismatched_upper_bound_still_flagged() {
        // The ceiling `0x1FF` (511) exceeds u8's max, so values 256..=511 fall
        // through to the wildcard and `val as u8` truncates — still flagged.
        let src = "fn f(x: i16) -> u8 { match x { \
                   val if val < 0 => 0, \
                   val if val > 0x1FF => 0xFF, \
                   val => val as u8 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6150_guarded_wildcard_still_flagged() {
        // The cast's own arm carries a guard, so it is not the catch-all the
        // elimination proof relies on — the exemption must not apply.
        let src = "fn f(x: i16) -> u8 { match x { \
                   val if val < 0 => 0, \
                   val if val > 0xFF => 0xFF, \
                   val if val % 2 == 0 => val as u8, \
                   val => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6150_shadowed_operand_in_wildcard_body_still_flagged() {
        // A `let val` shadow in the wildcard body rebinds the operand to an
        // out-of-range value, breaking the sibling-arm proof — still flagged.
        let src = "fn f(x: i16) -> u8 { match x { \
                   val if val < 0 => 0, \
                   val if val > 0xFF => 0xFF, \
                   val => { let val = 1000; val as u8 } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6150_signed_target_still_flagged() {
        // The exemption is unsigned-target-only: `0..=0xFF` does not fit `i8`
        // (max 127), so values 128..=255 overflow and the cast stays flagged.
        let src = "fn f(x: i16) -> i8 { match x { \
                   val if val < 0 => 0, \
                   val if val > 0xFF => 0x7F, \
                   val => val as i8 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn repro_6150_mismatched_binding_identifier_still_flagged() {
        // The bound arms bind `y` but the wildcard casts `val`; with no guard
        // referencing `val`, the bounds do not constrain it — still flagged.
        let src = "fn f(x: i16) -> u8 { match x { \
                   y if y < 0 => 0, \
                   y if y > 0xFF => 0xFF, \
                   val => val as u8 } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
