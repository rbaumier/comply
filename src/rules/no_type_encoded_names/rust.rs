//! no-type-encoded-names backend for Rust.
//!
//! Flags identifiers that encode their type in the name Hungarian-style:
//! `str_name`, `arr_items`, `bool_flag`, `i_count`. Rust's type system
//! already knows the type — the prefix is redundant and lies when the
//! type changes.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "identifier" {
        return;
    }
    if !is_declaration_site(node) {
        return;
    }
    let Ok(name) = node.utf8_text(source) else {
        return;
    };
    let Some(prefix) = super::type_prefix::matched_snake_case(name) else {
        return;
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-type-encoded-names".into(),
        message: format!(
            "'{name}' encodes a type prefix '{prefix}' — Hungarian notation is \
             obsolete. Remove the prefix; the type system already tells you the type."
        ),
        severity: Severity::Warning,
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
    fn flags_str_prefix() {
        assert_eq!(run_on("fn f() { let str_name = String::new(); }").len(), 1);
    }

    #[test]
    fn flags_arr_prefix() {
        assert_eq!(run_on("fn f() { let arr_items = vec![]; }").len(), 1);
    }

    #[test]
    fn flags_bool_prefix() {
        assert_eq!(run_on("fn f() { let bool_flag = true; }").len(), 1);
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
}
