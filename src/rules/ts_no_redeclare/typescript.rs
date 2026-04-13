//! ts-no-redeclare backend — detect duplicate variable declarations in the
//! same block scope (e.g. two `let x` or `var x` in the same function/block).
//!
//! Walks `variable_declarator` nodes, groups by their enclosing scope
//! (nearest function/block/program ancestor), and flags duplicates.
//! Allows TS declaration merging (interface + namespace, etc.) by only
//! checking `var`/`let`/`const` declarations.

use std::collections::HashMap;
use crate::diagnostic::{Diagnostic, Severity};

/// Find the enclosing scope node id for a given node.
fn scope_id(node: tree_sitter::Node) -> usize {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "program" | "function_declaration" | "function" | "arrow_function"
            | "method_definition" | "statement_block" => return p.id(),
            _ => {}
        }
        cur = p.parent();
    }
    // fallback: root
    0
}

/// Check if a node is inside a `var`/`let`/`const` declaration (not a type/interface).
fn is_variable_declaration(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "variable_declaration" | "lexical_declaration" => return true,
            "program" | "statement_block" | "function_declaration" | "function"
            | "arrow_function" | "export_statement" => return false,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    // Collect all variable_declarator names grouped by scope.
    // scope_id -> { name -> [positions] }
    let mut scopes: HashMap<usize, HashMap<String, Vec<tree_sitter::Point>>> = HashMap::new();

    fn collect_declarations(
        n: tree_sitter::Node,
        source: &[u8],
        scopes: &mut HashMap<usize, HashMap<String, Vec<tree_sitter::Point>>>,
    ) {
        if n.kind() == "variable_declarator" {
            if !is_variable_declaration(n) {
                return;
            }
            if let Some(name_node) = n.child_by_field_name("name")
                && name_node.kind() == "identifier"
                    && let Ok(name) = name_node.utf8_text(source) {
                        let sid = scope_id(n);
                        scopes
                            .entry(sid)
                            .or_default()
                            .entry(name.to_string())
                            .or_default()
                            .push(name_node.start_position());
                    }
            return; // don't recurse into children
        }

        let mut cursor = n.walk();
        for child in n.named_children(&mut cursor) {
            collect_declarations(child, source, scopes);
        }
    }

    collect_declarations(node, source, &mut scopes);

    for scope_map in scopes.values() {
        for (name, positions) in scope_map {
            if positions.len() > 1 {
                // Flag all after the first
                for pos in &positions[1..] {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "ts-no-redeclare".into(),
                        message: format!("`{name}` is already defined."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_var() {
        let d = run_on("var x = 1; var x = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_different_scopes() {
        let d = run_on("function a() { let x = 1; } function b() { let x = 2; }");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_duplicate_let_in_same_block() {
        let d = run_on("{ let y = 1; let y = 2; }");
        assert_eq!(d.len(), 1);
    }
}
