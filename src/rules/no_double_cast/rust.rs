//! no-double-cast Rust backend — flag `x as u32 as u64` chained casts.
//!
//! A double cast whose inner cast target is a raw pointer type
//! (`<expr> as *raw as <ptr|usize|...>`) is exempt: it is a pointer
//! reinterpretation / address extraction (repr(transparent) reinterpret,
//! byte-pointer, fn-pointer-to-address, FFI `c_void` erasure), not a numeric
//! "misaligned type" double cast. Rust forbids the single-step form in those
//! cases, so the two-step chain is mandatory and has no `From`/`Into`
//! alternative.

use crate::diagnostic::{Diagnostic, Severity};

/// Bit width of a primitive integer type by its `primitive_type` node text.
/// `isize`/`usize` map to 64: pointer width is target-dependent, but 64-bit is
/// the dominant target, and the truncate-then-widen carve-out only needs a
/// consistent ordering between the intermediate and final types.
fn int_width(name: &str) -> Option<u8> {
    Some(match name {
        "i8" | "u8" => 8,
        "i16" | "u16" => 16,
        "i32" | "u32" => 32,
        "i64" | "u64" | "isize" | "usize" => 64,
        "i128" | "u128" => 128,
        _ => return None,
    })
}

/// The integer truncate-then-widen chain `x as u16 as u32`: cast to a strictly
/// narrower intermediate (a deliberate, lossy truncation) then widen for
/// bit-packing. `x as u16 as u32 != x as u32` whenever the source exceeds the
/// intermediate width, so the chain is load-bearing, not redundant — and it
/// cannot be expressed with `From`/`Into` (those are infallible and non-lossy).
/// Only a strictly narrower intermediate qualifies: same-width (`u32 as u32`)
/// and widen-then-truncate (`u32 as u16`) chains are not this idiom.
fn int_truncate_then_widen(inner_ty: tree_sitter::Node, outer_ty: tree_sitter::Node, source: &[u8]) -> bool {
    if inner_ty.kind() != "primitive_type" || outer_ty.kind() != "primitive_type" {
        return false;
    }
    let (Ok(inner_name), Ok(outer_name)) = (inner_ty.utf8_text(source), outer_ty.utf8_text(source))
    else {
        return false;
    };
    let (Some(inner_w), Some(outer_w)) = (int_width(inner_name), int_width(outer_name)) else {
        return false;
    };
    inner_w < outer_w
}

crate::ast_check! { on ["type_cast_expression"] => |node, source, ctx, diagnostics|
    // The inner expression (left side of `as`) is the first named child.
    let Some(inner) = node.child_by_field_name("value") else { return };
    if inner.kind() != "type_cast_expression" {
        return;
    }
    let Some(inner_ty) = inner.child_by_field_name("type") else { return };

    // A cast chained off a raw pointer (`<expr> as *raw as <ptr|usize|...>`) is a
    // pointer reinterpretation / address extraction (repr(transparent) reinterpret,
    // byte-pointer, fn-pointer-to-address, FFI `c_void` erasure), not a numeric
    // "misaligned type" double cast. Rust forbids the single-step form, so the
    // two-step chain is mandatory and has no `From`/`Into` alternative. Exempt it.
    if inner_ty.kind() == "pointer_type" {
        return;
    }

    // Exempt the integer truncate-then-widen chain `x as u16 as u32`: the inner
    // cast to a strictly narrower type is a deliberate, lossy truncation, so the
    // chain is not collapsible to a single `as`.
    if let Some(outer_ty) = node.child_by_field_name("type")
        && int_truncate_then_widen(inner_ty, outer_ty, source)
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-double-cast".into(),
        message: "Double cast `as X as Y` hides misaligned types. \
                  Fix the real problem: align the types or use `From`/`Into`.".into(),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_double_as_cast() {
        // Same-width chain: genuinely redundant, the intermediate adds nothing.
        assert_eq!(run_on("fn f(x: i8) { let _ = x as u32 as u32; }").len(), 1);
    }

    #[test]
    fn allows_fat_to_thin_pointer_downcast() {
        // polars row_encoded.rs: `&dyn Trait` is a fat pointer; narrowing it to a
        // thin `*const Concrete` requires the intermediate `*const dyn Trait` cast.
        let src = "unsafe fn f(dyn_grouper: &dyn Grouper) { \
                   let _ = &*(dyn_grouper as *const dyn Grouper as *const RowEncodedHashGrouper); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_redundant_numeric_double_cast() {
        // Negative-space guard: a same-width numeric double cast still fires.
        assert_eq!(run_on("fn f(x: u16) { let _ = x as u32 as u32; }").len(), 1);
    }

    #[test]
    fn allows_truncate_then_widen_chain() {
        // rust-analyzer output.rs: `kind as u16 as u32` truncates an enum/wide
        // integer to the bitfield width, then widens for bit-packing. The u16 is
        // load-bearing (`x as u16 as u32 != x as u32` past u16), so this is exempt.
        assert!(run_on("fn f(kind: u32) -> u32 { kind as u16 as u32 }").is_empty());
    }

    #[test]
    fn allows_enum_truncate_then_widen_chain() {
        let src = "enum E { A } fn f() -> u32 { E::A as u16 as u32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_u8_to_u64_truncate_then_widen() {
        assert!(run_on("fn f(x: u32) -> u64 { x as u8 as u64 }").is_empty());
    }

    #[test]
    fn flags_same_width_integer_chain() {
        // Same width (32 == 32): the intermediate truncates nothing, so the
        // chain is genuinely redundant and still fires.
        assert_eq!(run_on("fn f(x: u32) { let _ = x as u32 as u32; }").len(), 1);
    }

    #[test]
    fn flags_widen_then_truncate_integer_chain() {
        // Inner wider than outer (32 > 16): not the truncate-then-widen idiom,
        // so it still fires.
        assert_eq!(run_on("fn f(x: u32) { let _ = x as u32 as u16; }").len(), 1);
    }

    #[test]
    fn allows_ref_to_typed_ptr_to_c_void_chain() {
        // helix faccess.rs: `&mut T as *mut _ as *mut c_void` is the mandatory
        // FFI erasure chain for passing a typed reference to a `*mut c_void` API.
        let src = "unsafe fn f() { \
                   let acl_info_ptr: *mut c_void = &mut acl_info as *mut _ as *mut c_void; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_ref_to_typed_ptr_to_c_void_chain_inferred_target() {
        let src = "unsafe fn f() { let mut ptr = &mut ace as *mut _ as *mut c_void; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_const_ref_to_const_c_void_chain() {
        let src = "unsafe fn f() { let p = &val as *const _ as *const c_void; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_ref_to_typed_ptr_to_path_qualified_c_void_chain() {
        let src = "unsafe fn f() { let p = &mut ace as *mut ACL as *mut core::ffi::c_void; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_numeric_intermediate_to_c_void() {
        // Negative-space guard: the inner cast target is `usize` (numeric), not a
        // raw pointer, so the pointer-chain exemption does not apply — still a
        // suspicious double cast.
        assert_eq!(run_on("fn f(x: usize) { let _ = x as usize as *mut c_void; }").len(), 1);
    }

    #[test]
    fn allows_repr_transparent_reinterpret_chain() {
        // repr(transparent) raw-pointer reinterpret: inner cast target is a raw
        // pointer, so the chain is a pointer reinterpretation, not numeric.
        let src = "unsafe fn f(t: *const u8) { let _ = t as *const u8 as *const u32; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_ref_to_typed_ptr_to_byte_ptr_chain() {
        // `&mut view as *mut _ as *mut u8`: reference -> typed raw ptr -> byte ptr.
        let src = "unsafe fn f() { let p = &mut view as *mut _ as *mut u8; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fn_pointer_to_address_chain() {
        // `signal_handler as *const () as usize`: function pointer -> address.
        let src = "fn f() { let addr = signal_handler as *const () as usize; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_field_base_typed_ptr_to_c_void_chain() {
        // diesel pg/connection/raw.rs: `self.value as *mut pgNotify as *mut c_void`,
        // base is a reference-typed `field_expression`, not a syntactic `&x`.
        let src = "unsafe fn f(g: G) { let p = g.value as *mut pgNotify as *mut core::ffi::c_void; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("fn f(x: i32) { let _ = x as u64; }").is_empty());
    }

    #[test]
    fn flags_triple_cast() {
        // Each adjacent pair is same-width or widen-then-truncate (never the
        // narrowing-then-widen idiom), so the chain stays flagged.
        let d = run_on("fn f(x: i8) { let _ = x as u32 as u32 as u16; }");
        // The outer cast and the middle cast are both flagged.
        assert!(!d.is_empty());
    }
}
