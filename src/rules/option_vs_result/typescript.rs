//! option-vs-result — find*/get* functions returning null/undefined.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the identifier starts with `find` or `get` followed by
/// an uppercase letter (camelCase convention).
fn is_find_or_get(name: &str) -> bool {
    for prefix in &["find", "get"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && rest.starts_with(|c: char| c.is_ascii_uppercase()) {
                return true;
            }
    }
    false
}

/// Returns true if a `return_statement` node returns `null` or `undefined`.
fn is_null_return(node: tree_sitter::Node, _source: &[u8]) -> bool {
    if node.kind() != "return_statement" {
        return false;
    }
    // The returned expression is the second child (first is `return` keyword).
    let Some(expr) = node.child(1) else { return false };
    matches!(expr.kind(), "null" | "undefined")
}

/// Recursively check if any descendant is a null/undefined return.
fn has_null_return(node: tree_sitter::Node, source: &[u8]) -> bool {
    if is_null_return(node, source) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Don't descend into nested function bodies.
        match child.kind() {
            "function_declaration" | "function" | "arrow_function"
            | "generator_function_declaration" | "generator_function"
            | "method_definition" => continue,
            _ => {}
        }
        if has_null_return(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match function declarations and variable-assigned arrow/function exprs.
    let fn_name = match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            name_node.utf8_text(source).unwrap_or("")
        }
        "method_definition" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            name_node.utf8_text(source).unwrap_or("")
        }
        "variable_declarator" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Some(value) = node.child_by_field_name("value") else { return };
            if value.kind() != "arrow_function" && value.kind() != "function" {
                return;
            }
            name_node.utf8_text(source).unwrap_or("")
        }
        _ => return,
    };

    if !is_find_or_get(fn_name) {
        return;
    }

    // Check the function body for `return null` or `return undefined`.
    let Some(body) = node.child_by_field_name("body") else { return };
    if !has_null_return(body, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "option-vs-result".into(),
        message: "Function named `find*`/`get*` returns `null`/`undefined` — \
                  consider using an Option type to make absence explicit."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_find_returning_null() {
        let src = r#"
function findUser(id: string) {
    if (!id) return null;
    return db.get(id);
}
"#;
        let d = run_ts(src, &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "option-vs-result");
    }

    #[test]
    fn flags_get_returning_undefined() {
        let src = r#"
function getConfig(key: string) {
    if (!map.has(key)) return undefined;
    return map.get(key);
}
"#;
        assert_eq!(run_ts(src, &Check).len(), 1);
    }

    #[test]
    fn allows_find_without_null_return() {
        let src = r#"
function findUser(id: string) {
    return db.get(id);
}
"#;
        assert!(run_ts(src, &Check).is_empty());
    }

    #[test]
    fn ignores_non_find_get_functions() {
        let src = r#"
function createUser(name: string) {
    if (!name) return null;
    return { name };
}
"#;
        assert!(run_ts(src, &Check).is_empty());
    }
}
