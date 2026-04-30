use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { on ["function_declaration", "function_expression", "method_definition", "arrow_function"] => |node, source, ctx, diagnostics|
    let body = match node.kind() {
        "function_declaration" | "function_expression" | "method_definition" => {
            node.child_by_field_name("body")
        }
        "arrow_function" => {
            // Skip arrow functions with expression body (single return type)
            let body = node.child_by_field_name("body");
            if body.map(|b| b.kind() != "statement_block").unwrap_or(true) {
                return;
            }
            body
        }
        _ => return,
    };

    let Some(body) = body else { return; };

    // Collect return statement value types
    let mut return_types: HashSet<&str> = HashSet::new();
    collect_return_types(body, source, &mut return_types);

    // Skip if less than 2 different types or if empty
    if return_types.len() < 2 { return; }

    // Skip common valid patterns (null/undefined with value)
    let has_null_or_undefined = return_types.contains("null") || return_types.contains("undefined");
    let non_nullish: Vec<_> = return_types.iter()
        .filter(|&&t| t != "null" && t != "undefined")
        .collect();

    // If it's just value + null/undefined, that's a common nullable pattern
    if has_null_or_undefined && non_nullish.len() <= 1 { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "function-return-type".into(),
        message: format!("Function returns inconsistent types: {:?}", return_types),
        severity: Severity::Warning,
        span: None,
    });
}

fn collect_return_types<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    types: &mut HashSet<&'a str>,
) {
    if node.kind() == "return_statement"
        && let Some(value) = node.named_child(0)
    {
        let type_hint = infer_type(value, source);
        types.insert(type_hint);
    }

    // Don't descend into nested functions
    if node.kind() == "function_declaration"
        || node.kind() == "function_expression"
        || node.kind() == "arrow_function"
    {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_return_types(child, source, types);
    }
}

fn infer_type<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    match node.kind() {
        "number" => "number",
        "string" | "template_string" => "string",
        "true" | "false" => "boolean",
        "null" => "null",
        "undefined" => "undefined",
        "array" => "array",
        "object" => "object",
        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("");
            if text == "undefined" {
                "undefined"
            } else {
                "unknown"
            }
        }
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(code, &Check)
    }

    #[test]
    fn flags_string_or_number() {
        let code = "function f(x) { if (x) return 'a'; return 1; }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_object_or_array() {
        let code = "function f(x) { if (x) return {}; return []; }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_nullable() {
        let code = "function f(x) { if (x) return 'a'; return null; }";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_consistent_type() {
        let code = "function f(x) { if (x) return 1; return 2; }";
        assert!(run(code).is_empty());
    }
}
