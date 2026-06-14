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

crate::ast_check! { on ["type_cast_expression"] => |node, _source, ctx, diagnostics|
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
        assert_eq!(run_on("fn f(x: i8) { let _ = x as u32 as u64; }").len(), 1);
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
        // Negative-space guard: a genuinely redundant numeric double cast still fires.
        assert_eq!(run_on("fn f(x: u16) { let _ = x as u32 as u64; }").len(), 1);
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("fn f(x: i32) { let _ = x as u64; }").is_empty());
    }

    #[test]
    fn flags_triple_cast() {
        let d = run_on("fn f(x: i8) { let _ = x as i16 as u32 as u64; }");
        // The outer cast and the middle cast are both flagged.
        assert!(!d.is_empty());
    }
}
