//! no-type-encoded-names backend — flag identifiers encoding the variable's
//! type in the name: `strName`, `arrItems`, `boolReady`, `iCount`, `objUser`.
//!
//! Why: the TypeScript type system already tells you what `name` is —
//! adding `str` to the identifier is Hungarian notation, which was obsolete
//! the moment we got type checkers. Worse, the prefix lies when the type
//! changes: `strCount` becomes a number and nobody notices.
//!
//! Detection: walk identifier declarations and check if the name starts
//! with a type-prefix followed by a camelCase boundary (uppercase letter).

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
    let Some(prefix) = super::type_prefix::matched_camel_case(name) else {
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
             obsolete. Remove the prefix; TypeScript's type checker already \
             knows the type."
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
        "variable_declarator" | "required_parameter" | "function_declaration"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_str_prefix() {
        assert_eq!(run_on("const strName = 'x';").len(), 1);
    }

    #[test]
    fn flags_arr_prefix() {
        assert_eq!(run_on("const arrItems = [];").len(), 1);
    }

    #[test]
    fn flags_bool_prefix() {
        assert_eq!(run_on("const boolReady = true;").len(), 1);
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("const userName = 'x';").is_empty());
        assert!(run_on("const items = [];").is_empty());
        assert!(run_on("const isReady = true;").is_empty());
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // 'string' starts with 'str' but there's no camelCase boundary.
        assert!(run_on("const string = 'x';").is_empty());
        // 'array' starts with 'arr' but 'a' is lowercase after.
        assert!(run_on("const arrayList = 1;").is_empty());
    }

    #[test]
    fn does_not_flag_descriptive_fn_callback() {
        // `fn` was previously in the prefix list; flagging `fnCallback`
        // is wrong because it's a descriptive name for a function-typed
        // variable, not Hungarian for some primitive type.
        assert!(run_on("const fnCallback = () => {};").is_empty());
    }

    #[test]
    fn does_not_flag_num_items() {
        // `num_items` / `numItems` is "number of items", not Hungarian
        // for a primitive number variable.
        assert!(run_on("const numItems = 5;").is_empty());
    }

    #[test]
    fn does_not_flag_int_count() {
        // TypeScript has no `int` type — `intCount` is descriptive.
        assert!(run_on("const intCount = 0;").is_empty());
    }

    #[test]
    fn flags_legacy_dbl_prefix() {
        // `dbl` is a legacy C/C++ Hungarian prefix for `double`.
        assert_eq!(run_on("const dblValue = 3.14;").len(), 1);
    }
}
