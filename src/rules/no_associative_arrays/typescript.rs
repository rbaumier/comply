//! no-associative-arrays backend — flag string-keyed assignment on arrays.
//!
//! Detects patterns like:
//!   const arr = [];
//!   arr["key"] = 1;

use crate::diagnostic::{Diagnostic, Severity};

/// Extract text from a tree-sitter node.
fn text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// Check if a value node is an array literal, `new Array(...)`, or typed `Array<T>`.
fn is_array_init(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "array" => true,
        "new_expression" => {
            node.child_by_field_name("constructor")
                .is_some_and(|c| text(c, source) == "Array")
        }
        _ => false,
    }
}

/// Check if a subscript index is a string literal (single or double quoted).
fn is_string_index(node: tree_sitter::Node) -> bool {
    node.kind() == "string"
}

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    // Look for assignment expressions: arr["key"] = value
    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "subscript_expression" {
        return;
    }

    // The index must be a string literal.
    let Some(index) = left.child_by_field_name("index") else { return };
    if !is_string_index(index) {
        return;
    }

    // The object being subscripted.
    let Some(obj) = left.child_by_field_name("object") else { return };
    if obj.kind() != "identifier" {
        return;
    }
    let var_name = text(obj, source);

    // Walk up to find the enclosing scope (program or statement_block) and
    // look for a variable declaration that initialises this name as an array.
    let mut scope = node.parent();
    while let Some(s) = scope {
        match s.kind() {
            "program" | "statement_block" => break,
            _ => scope = s.parent(),
        }
    }
    let Some(scope_node) = scope else { return };

    let mut cursor = scope_node.walk();
    let mut found_array_decl = false;
    for child in scope_node.children(&mut cursor) {
        if child.kind() != "variable_declaration" && child.kind() != "lexical_declaration" {
            continue;
        }
        let mut inner = child.walk();
        for declarator in child.children(&mut inner) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_node) = declarator.child_by_field_name("name") else { continue };
            if text(name_node, source) != var_name {
                continue;
            }
            if let Some(value) = declarator.child_by_field_name("value")
                && is_array_init(value, source) {
                    found_array_decl = true;
                }
        }
    }

    if !found_array_decl {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-associative-arrays".into(),
        message: format!(
            "Array `{var_name}` is used as an associative array — use a Map or plain object instead."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bracket_string_key_assignment() {
        let src = "const arr = [];\narr[\"key\"] = 1;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_single_quote_bracket_key() {
        let src = "let items = [];\nitems['name'] = \"hello\";";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_numeric_index() {
        let src = "const arr = [];\narr[0] = 1;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_array_bracket_access() {
        let src = "const obj = {};\nobj[\"key\"] = 1;";
        assert!(run_on(src).is_empty());
    }
}
