//! ts-explicit-module-boundary-types backend — flag exported functions
//! whose parameters or return type are inferred.
//!
//! Detection: find any `export_statement`, then look at the exported
//! entity. If it is a function_declaration, check its params + return type.
//! If it is a `lexical_declaration` binding an arrow_function /
//! function_expression, check that too.
//!
//! Class members are not covered by this rule — use
//! `ts-explicit-function-return-type` or `ts-explicit-member-accessibility`
//! for class surface.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let root = tree.root_node();
        let mut diagnostics = Vec::new();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() != "export_statement" {
                continue;
            }
            check_export(child, source_bytes, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

fn check_export(
    export: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => check_function_node(child, source, ctx, diagnostics),
            "lexical_declaration" => {
                check_lexical_declaration(child, source, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_lexical_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else { continue };
        if value.kind() == "arrow_function" || value.kind() == "function_expression" {
            check_function_node(value, source, ctx, diagnostics);
        }
    }
}

fn check_function_node(
    func: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let name = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");
    let pos = func.start_position();

    // Return type check — direct `type_annotation` child of the function.
    if !has_return_type(func) {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-explicit-module-boundary-types".into(),
            message: format!(
                "Exported function '{name}' is missing a return type annotation."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }

    // Parameter types — walk `parameters` looking for untyped params.
    if let Some(params) = func.child_by_field_name("parameters") {
        let mut pcursor = params.walk();
        for param in params.named_children(&mut pcursor) {
            if !param_has_type(param) {
                let param_name = extract_param_name(param, source).unwrap_or("<param>");
                let ppos = param.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: ppos.row + 1,
                    column: ppos.column + 1,
                    rule_id: "ts-explicit-module-boundary-types".into(),
                    message: format!(
                        "Exported function '{name}' parameter '{param_name}' \
                         is missing a type annotation."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn has_return_type(func: tree_sitter::Node) -> bool {
    let mut cursor = func.walk();
    func.children(&mut cursor)
        .any(|c| c.kind() == "type_annotation")
}

fn param_has_type(param: tree_sitter::Node) -> bool {
    let mut cursor = param.walk();
    param
        .children(&mut cursor)
        .any(|c| c.kind() == "type_annotation")
}

fn extract_param_name<'a>(param: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = param.walk();
    for child in param.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_missing_return_type() {
        let diags = run_on("export function foo(a: number) { return a; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("return type"));
    }

    #[test]
    fn flags_missing_param_type() {
        let diags = run_on("export function foo(a): number { return 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("parameter"));
    }

    #[test]
    fn allows_fully_typed_export() {
        assert!(run_on("export function foo(a: number): number { return a; }").is_empty());
    }

    #[test]
    fn does_not_flag_non_exported_function() {
        assert!(run_on("function helper(a) { return a; }").is_empty());
    }

    #[test]
    fn flags_exported_arrow_without_types() {
        let diags = run_on("export const foo = (a) => a;");
        // Missing return type + missing param type = 2 diagnostics.
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_typed_exported_arrow() {
        assert!(run_on("export const foo = (a: number): number => a;").is_empty());
    }
}
