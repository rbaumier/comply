//! no-type-encoded-names backend for Rust.
//!
//! Flags identifiers that encode their type in the name Hungarian-style:
//! `str_name`, `arr_items`, `bool_flag`, `i_count`. Rust's type system
//! already knows the type — the prefix is redundant and lies when the
//! type changes.

use crate::diagnostic::{Diagnostic, Severity};

const RUST_DOMAIN_PREFIXES: &[&str] = &["str", "arr", "bool", "obj"];

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

fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "let_declaration" | "parameter" | "function_item" | "const_item" | "static_item"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
}
