//! no-type-encoded-names backend for Rust.
//!
//! Flags identifiers that encode their type in the name Hungarian-style:
//! `str_name`, `arr_items`, `bool_flag`, `i_count`. Rust's type system
//! already knows the type — the prefix is redundant and lies when the
//! type changes.

use crate::diagnostic::{Diagnostic, Severity};

const RUST_DOMAIN_PREFIXES: &[&str] = &["str", "arr", "bool"];

crate::ast_check! { on ["identifier"] => |node, source, ctx, diagnostics|
    if !is_declaration_site(node) {
        return;
    }
    let Ok(name) = node.utf8_text(source) else {
        return;
    };
    let Some(prefix) = super::type_prefix::matched_snake_case(name) else {
        return;
    };
    if RUST_DOMAIN_PREFIXES.contains(&prefix) {
        return;
    }
    // A type-abbreviation prefix is only Hungarian notation when the binding's
    // actual type matches the type the prefix claims. When the declaration
    // carries an explicit type annotation that contradicts the prefix — a
    // generic `&T`, a reference, a non-float for `flt`/`dbl` — the prefix
    // names a domain concept (`flt_id: &T` = VCF/BCF *filter* id), not the
    // type, so it is not type-encoding. Bindings with no annotation keep
    // firing on the prefix alone.
    if let Some(ty) = explicit_type_annotation(node, source)
        && !prefix_matches_type(prefix, ty)
    {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-type-encoded-names".into(),
        message: format!(
            "'{name}' encodes a type prefix '{prefix}' — Hungarian notation is \
             obsolete. Remove the prefix; the type system already tells you the type."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// Hungarian notation encodes the type of a *value* (variable, parameter,
/// const, static). A function name is an operation verb, not a value, so a
/// type-abbreviation prefix on a function name is the operation it performs
/// (`dbl_in_place` = the doubling operation, `str_split` = split a string),
/// not a type encoding. Excluding `function_item` keeps the Rust backend in
/// step with the TypeScript one, which only inspects value bindings.
fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "let_declaration" | "parameter" | "const_item" | "static_item"
    )
}

/// The text of the binding's explicit type annotation, if any. All four
/// declaration sites expose it as the `type` field (`let x: T`, `p: T`,
/// `const C: T`, `static S: T`); a `let` written without `: T` yields `None`.
fn explicit_type_annotation<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.parent()?
        .child_by_field_name("type")?
        .utf8_text(source)
        .ok()
}

const FLOAT_TYPES: &[&str] = &["f32", "f64"];
const INT_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];

/// Whether an explicit annotation `ty` is the primitive type a legacy
/// Hungarian prefix claims the value has. A genuine type-encoded name annotates
/// the matching primitive (`flt_total: f64`); a domain abbreviation annotates
/// something else (`flt_id: &T`), so the prefix is not a redundant type tag.
/// `str`/`arr`/`bool` never reach here — they are filtered out earlier.
fn prefix_matches_type(prefix: &str, ty: &str) -> bool {
    let ty = ty.trim();
    match prefix {
        "flt" | "dbl" => FLOAT_TYPES.contains(&ty),
        "lng" => INT_TYPES.contains(&ty),
        "chr" => ty == "char",
        "byt" => ty == "u8",
        // `prom` (Promise) has no Rust primitive, so an explicit annotation can
        // never confirm it — an annotated `prom_*` is a domain name.
        "prom" => false,
        // Unknown prefix: keep the pre-annotation behaviour (flag on prefix).
        _ => true,
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

    #[test]
    fn allows_str_prefix_domain_qualifier() {
        assert!(run_on("fn f() { let str_name = String::new(); }").is_empty());
    }

    #[test]
    fn allows_arr_prefix_domain_qualifier() {
        assert!(run_on("fn f() { let arr_items = vec![]; }").is_empty());
    }

    #[test]
    fn allows_bool_prefix_domain_qualifier() {
        assert!(run_on("fn f() { let bool_flag = true; }").is_empty());
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("fn f() { let user_name = String::new(); }").is_empty());
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // `string` and `array` start with str/arr but without underscore.
        assert!(run_on("fn f() { let strawberry = 1; }").is_empty());
        assert!(run_on("fn f() { let array_of_things = vec![]; }").is_empty());
    }

    #[test]
    fn does_not_flag_fn_name() {
        // The original false positive: `fn_name` literally means
        // "function name", and `fn` is also a Rust keyword. Flagging
        // it as Hungarian-prefixed is wrong.
        assert!(run_on("fn f() { let fn_name = String::new(); }").is_empty());
    }

    #[test]
    fn does_not_flag_func_callback() {
        assert!(run_on("fn f() { let func_callback = || {}; }").is_empty());
    }

    #[test]
    fn does_not_flag_num_items() {
        // `num_items` is "number of items", not Hungarian for a u64.
        assert!(run_on("fn f() { let num_items = 5; }").is_empty());
    }

    #[test]
    fn does_not_flag_int_count() {
        // Rust has no `int` type. `int_count` is descriptive prose.
        assert!(run_on("fn f() { let int_count = 0; }").is_empty());
    }

    #[test]
    fn does_not_flag_vec_indices() {
        // `vec_indices` reads as "vector of indices" in Rust prose.
        assert!(run_on("fn f() { let vec_indices: Vec<usize> = vec![]; }").is_empty());
    }

    #[test]
    fn flags_legacy_dbl_prefix() {
        assert_eq!(run_on("fn f() { let dbl_value = 3.14; }").len(), 1);
    }

    #[test]
    fn does_not_flag_dbl_operation_function_name() {
        // #5779: `dbl_in_place` is the doubling operation (left-shift by 1)
        // in bignum/Montgomery arithmetic, parallel to `add_in_place` /
        // `mul_in_place`. The `dbl` prefix is the verb, not a `double` type
        // encoding on a value. Function names are operations, not typed values.
        assert!(run_on("fn dbl_in_place(x: &mut u64) { *x <<= 1; }").is_empty());
        assert!(run_on("fn dbl_in_place_large(r: &mut Repr) {}").is_empty());
    }

    #[test]
    fn flags_dbl_prefixed_value_parameter() {
        // The exclusion is for function *names* only — a value parameter that
        // genuinely type-encodes is still flagged.
        assert_eq!(run_on("fn f(dbl_value: f64) {}").len(), 1);
    }

    #[test]
    fn does_not_flag_flt_prefix_on_non_float_generic_param() {
        // #6060: `flt` is the VCF/BCF *filter* abbreviation, not float
        // Hungarian. The annotation is a generic `&T`, not `f32`/`f64`, so the
        // prefix cannot be encoding a float type.
        assert!(
            run_on(
                "struct R; impl R { \
                 pub fn has_filter<T: FilterId + ?Sized>(&self, flt_id: &T) -> bool { false } }"
            )
            .is_empty()
        );
        assert!(
            run_on(
                "struct R; impl R { \
                 pub fn set_filters<T: FilterId + ?Sized>(&mut self, flt_ids: &[&T]) {} }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_genuine_float_hungarian_param() {
        // Strictness preserved: an explicit `f64` annotation confirms the `flt`
        // prefix is a redundant float type tag.
        assert_eq!(run_on("fn f(flt_total: f64) {}").len(), 1);
        assert_eq!(run_on("fn f() { let flt_total: f32 = 0.0; }").len(), 1);
    }
}
