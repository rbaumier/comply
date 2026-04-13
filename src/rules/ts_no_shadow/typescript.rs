//! ts-no-shadow backend — simplified variable shadowing detection.
//!
//! Walks the AST collecting variable declarations per scope. When a
//! variable is declared in an inner scope that already has a same-named
//! variable in an outer scope, it flags the inner declaration.
//!
//! Simplified: only checks `variable_declarator` names (let/const/var),
//! function parameters, and function declarations. Does not handle all
//! TS-specific cases like enum members or type declarations.

use std::collections::HashSet;
use crate::diagnostic::{Diagnostic, Severity};

/// Scope kinds that introduce a new variable scope.
fn is_scope_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "program"
            | "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "for_statement"
            | "for_in_statement"
            | "for_of_statement"
            | "statement_block"
            | "catch_clause"
    )
}

fn walk_scope(
    node: tree_sitter::Node,
    source: &[u8],
    outer_names: &HashSet<String>,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Collect names declared in this scope's immediate children
    let mut current_names: Vec<(String, tree_sitter::Point)> = Vec::new();

    // For function nodes, collect parameter names
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for child in params.named_children(&mut cursor) {
            match child.kind() {
                "required_parameter" | "optional_parameter" => {
                    if let Some(pat) = child.child_by_field_name("pattern")
                        && pat.kind() == "identifier"
                            && let Ok(name) = pat.utf8_text(source) {
                                current_names.push((name.to_string(), pat.start_position()));
                            }
                }
                "identifier" => {
                    if let Ok(name) = child.utf8_text(source) {
                        current_names.push((name.to_string(), child.start_position()));
                    }
                }
                _ => {}
            }
        }
    }

    // Walk direct children for variable declarations
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "lexical_declaration" || child.kind() == "variable_declaration" {
            let mut inner_cursor = child.walk();
            for decl in child.named_children(&mut inner_cursor) {
                if decl.kind() == "variable_declarator"
                    && let Some(name_node) = decl.child_by_field_name("name")
                        && name_node.kind() == "identifier"
                            && let Ok(name) = name_node.utf8_text(source) {
                                current_names.push((name.to_string(), name_node.start_position()));
                            }
            }
        }
    }

    // Check for shadowing
    for (name, pos) in &current_names {
        if outer_names.contains(name) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-no-shadow".into(),
                message: format!("`{name}` is already declared in an outer scope."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }

    // Build the set of names visible in nested scopes
    let mut combined: HashSet<String> = outer_names.clone();
    for (name, _) in &current_names {
        combined.insert(name.clone());
    }

    // Recurse into child scope boundaries
    let mut cursor2 = node.walk();
    for child in node.named_children(&mut cursor2) {
        if is_scope_boundary(child.kind()) {
            walk_scope(child, source, &combined, ctx, diagnostics);
        } else {
            // Recurse into non-scope nodes to find nested scopes
            let mut inner_cursor = child.walk();
            for grandchild in child.named_children(&mut inner_cursor) {
                fn find_scopes(
                    n: tree_sitter::Node,
                    source: &[u8],
                    outer: &HashSet<String>,
                    ctx: &crate::rules::backend::CheckCtx,
                    diags: &mut Vec<Diagnostic>,
                ) {
                    if is_scope_boundary(n.kind()) {
                        walk_scope(n, source, outer, ctx, diags);
                        return;
                    }
                    let mut c = n.walk();
                    for child in n.named_children(&mut c) {
                        find_scopes(child, source, outer, ctx, diags);
                    }
                }
                find_scopes(grandchild, source, &combined, ctx, diagnostics);
            }
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let outer: HashSet<String> = HashSet::new();
    walk_scope(node, source, &outer, ctx, diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_shadowed_variable() {
        let d = run_on("const x = 1; function f() { const x = 2; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_different_names() {
        assert!(run_on("const x = 1; function f() { const y = 2; }").is_empty());
    }

    #[test]
    fn flags_param_shadowing_outer() {
        let d = run_on("const x = 1; function f(x: number) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_nested_shadow() {
        let d = run_on("const a = 1; function f() { const a = 2; function g() { const a = 3; } }");
        assert!(d.len() >= 2);
    }
}
