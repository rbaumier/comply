//! no-double-cast Rust backend — flag `x as u32 as u64` chained casts.

use crate::diagnostic::{Diagnostic, Severity};

/// A raw pointer to a `dyn Trait`, i.e. a `*const dyn Trait` / `*mut dyn Trait`
/// fat pointer (data pointer + vtable).
fn raw_ptr_to_dyn(ty: tree_sitter::Node) -> bool {
    ty.kind() == "pointer_type"
        && ty
            .child_by_field_name("type")
            .is_some_and(|inner| inner.kind() == "dynamic_type")
}

/// A raw pointer to a non-`dyn` type, i.e. a thin pointer `*const T` / `*mut T`.
fn raw_ptr_to_thin(ty: tree_sitter::Node) -> bool {
    ty.kind() == "pointer_type"
        && ty
            .child_by_field_name("type")
            .is_some_and(|inner| inner.kind() != "dynamic_type")
}

/// A raw pointer to `c_void`, i.e. `*mut c_void` / `*const c_void`. Matches the
/// final type segment so path-qualified forms (`core::ffi::c_void`,
/// `std::os::raw::c_void`, `libc::c_void`) are recognized too.
fn raw_ptr_to_c_void(ty: tree_sitter::Node, source: &[u8]) -> bool {
    if ty.kind() != "pointer_type" {
        return false;
    }
    let Some(inner) = ty.child_by_field_name("type") else {
        return false;
    };
    let leaf = match inner.kind() {
        "type_identifier" => inner,
        // `core::ffi::c_void` etc.: the trailing `type_identifier` is the name.
        "scoped_type_identifier" => match inner.child_by_field_name("name") {
            Some(name) => name,
            None => return false,
        },
        _ => return false,
    };
    leaf.utf8_text(source).is_ok_and(|name| name == "c_void")
}

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

/// The alignment-preserving FFI erasure chain `&x as *mut _ as *mut c_void`:
/// a reference coerced to a typed raw pointer, then type-erased to a void
/// pointer. Rust forbids casting `&T`/`&mut T` straight to `*mut c_void`, so the
/// intermediate typed raw pointer is mandatory, not a hidden misalignment.
fn ref_to_void_ptr_chain(
    inner: tree_sitter::Node,
    outer_ty: tree_sitter::Node,
    source: &[u8],
) -> bool {
    raw_ptr_to_c_void(outer_ty, source)
        && inner
            .child_by_field_name("type")
            .is_some_and(raw_ptr_to_thin)
        && inner
            .child_by_field_name("value")
            .is_some_and(|base| base.kind() == "reference_expression")
}

crate::ast_check! { on ["type_cast_expression"] => |node, source, ctx, diagnostics|
    // The inner expression (left side of `as`) is the first named child.
    let Some(inner) = node.child_by_field_name("value") else { return };
    if inner.kind() != "type_cast_expression" {
        return;
    }

    // Exempt the fat-pointer-to-thin-pointer downcast `x as *const dyn Trait
    // as *const Concrete`. Rust cannot convert a fat `*const dyn Trait` (which
    // carries a vtable) to a thin `*const Concrete` in a single `as`, so the
    // intermediate cast is required, not redundant.
    if let (Some(inner_ty), Some(outer_ty)) =
        (inner.child_by_field_name("type"), node.child_by_field_name("type"))
        && raw_ptr_to_dyn(inner_ty)
        && raw_ptr_to_thin(outer_ty)
    {
        return;
    }

    // Exempt the FFI erasure chain `&x as *mut _ as *mut c_void`. Rust forbids
    // casting a reference straight to `*mut c_void`, so the intermediate typed
    // raw pointer is mandatory; both casts preserve alignment.
    if let Some(outer_ty) = node.child_by_field_name("type")
        && ref_to_void_ptr_chain(inner, outer_ty, source)
    {
        return;
    }

    // Exempt the integer truncate-then-widen chain `x as u16 as u32`: the inner
    // cast to a strictly narrower type is a deliberate, lossy truncation, so the
    // chain is not collapsible to a single `as`.
    if let (Some(inner_ty), Some(outer_ty)) =
        (inner.child_by_field_name("type"), node.child_by_field_name("type"))
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
        // Negative-space guard: the inner cast is to a numeric type, not a raw
        // pointer, and the base is not a reference — still a suspicious double cast.
        assert_eq!(run_on("fn f(x: usize) { let _ = x as usize as *mut c_void; }").len(), 1);
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
