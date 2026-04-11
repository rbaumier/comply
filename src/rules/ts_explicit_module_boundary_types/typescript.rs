//! ts-explicit-module-boundary-types backend — flag exported functions that
//! lack explicit return type annotations.
//!
//! This rule focuses on module boundaries: `export function` / `export default`
//! / `export const fn = ...` declarations without return types.
//!
//! In tree-sitter-typescript, the return type is a `type_annotation` child
//! of the function node (after `formal_parameters`, before the body).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "export_statement" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                check_function_return_type(&child, ctx, diagnostics);
            }
            "lexical_declaration" => {
                let mut inner_cursor = child.walk();
                for decl in child.named_children(&mut inner_cursor) {
                    if decl.kind() == "variable_declarator"
                        && let Some(value) = decl.child_by_field_name("value")
                            && (value.kind() == "arrow_function" || value.kind() == "function") {
                                // Skip if the variable has a type annotation.
                                if decl.child_by_field_name("type").is_some() {
                                    continue;
                                }
                                check_function_return_type(&value, ctx, diagnostics);
                            }
                }
            }
            _ => {}
        }
    }
}

fn check_function_return_type(
    node: &tree_sitter::Node,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check for type_annotation child (the return type).
    let has_return_type = {
        let mut c = node.walk();
        node.children(&mut c).any(|ch| ch.kind() == "type_annotation")
    };
    if has_return_type {
        return;
    }

    // For arrow functions with concise body, skip.
    if node.kind() == "arrow_function" {
        let has_block_body = node
            .child_by_field_name("body")
            .map(|b| b.kind() == "statement_block")
            .unwrap_or(false);
        if !has_block_body {
            return;
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-explicit-module-boundary-types".into(),
        message: "Missing return type on exported function.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_exported_function_without_return_type() {
        let diags = run_on("export function foo() { return 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing return type"));
    }

    #[test]
    fn allows_exported_function_with_return_type() {
        let diags = run_on("export function foo(): number { return 1; }");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_exported_arrow_without_return_type() {
        let diags = run_on("export const foo = (x: number) => { return x; };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_exported_function() {
        let diags = run_on("function foo() { return 1; }");
        assert!(diags.is_empty());
    }
}
