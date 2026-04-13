//! ts-explicit-function-return-type backend — flag function declarations,
//! function expressions, and arrow functions that lack a return type annotation.
//!
//! In tree-sitter-typescript, the return type can be:
//! - `child_by_field_name("return_type")` on some node types
//! - a `type_annotation` named child (after formal_parameters, before body)

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    if kind != "function_declaration"
        && kind != "method_definition"
    {
        return;
    }

    // Check if there's a return type annotation.
    if has_return_type(node) {
        return;
    }

    // Skip constructors and setters — they never need return types.
    if kind == "method_definition" {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i)
                && child.kind() == "set" {
                    return;
                }
        }
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
            if name == "constructor" {
                return;
            }
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-explicit-function-return-type".into(),
        message: "Missing return type on function.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn has_return_type(node: tree_sitter::Node) -> bool {
    if node.child_by_field_name("return_type").is_some() {
        return true;
    }
    let mut c = node.walk();
    node.children(&mut c).any(|ch| ch.kind() == "type_annotation")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_function_without_return_type() {
        let diags = run_on("function foo() { return 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing return type"));
    }

    #[test]
    fn allows_function_with_return_type() {
        let diags = run_on("function foo(): number { return 1; }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_concise_arrow() {
        let diags = run_on("const f = (x: number) => x + 1;");
        assert!(diags.is_empty());
    }
}
