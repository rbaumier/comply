//! no-redeclare backend — detect duplicate `var` / `function` / `let`
//! / `const` declarations of the same name within the same lexical
//! scope (program, function body, or statement block).
//!
//! We check each scope node once (program, statement_block, function
//! body) and compare the identifiers of every direct declaration inside.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

fn scope_kinds(kind: &str) -> bool {
    matches!(
        kind,
        "program" | "statement_block" | "function_body"
    )
}

/// Collect identifier tokens for declarations that live DIRECTLY in this
/// scope (not inside a nested statement_block). Returns (name, line, col).
fn collect_declarations<'a>(
    scope: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Vec<(&'a str, usize, usize)> {
    let mut names: Vec<(&'a str, usize, usize)> = Vec::new();
    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        match child.kind() {
            "variable_declaration" | "lexical_declaration" => {
                let mut c2 = child.walk();
                for vc in child.children(&mut c2) {
                    if vc.kind() != "variable_declarator" {
                        continue;
                    }
                    if let Some(id) = vc.child_by_field_name("name")
                        && id.kind() == "identifier"
                        && let Ok(text) = id.utf8_text(source)
                    {
                        let pos = id.start_position();
                        names.push((text, pos.row + 1, pos.column + 1));
                    }
                }
            }
            "function_declaration" => {
                if let Some(id) = child.child_by_field_name("name")
                    && let Ok(text) = id.utf8_text(source)
                {
                    let pos = id.start_position();
                    names.push((text, pos.row + 1, pos.column + 1));
                }
            }
            _ => {}
        }
    }
    names
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !scope_kinds(node.kind()) {
        return;
    }

    let names = collect_declarations(node, source);
    let mut seen: HashMap<&str, (usize, usize)> = HashMap::new();
    for (name, line, col) in names {
        if seen.contains_key(name) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line,
                column: col,
                rule_id: "no-redeclare".into(),
                message: format!("`{name}` is already declared in this scope."),
                severity: Severity::Warning,
                span: None,
            });
        } else {
            seen.insert(name, (line, col));
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
        assert_eq!(run_on("var a = 1; var a = 2;").len(), 1);
    }

    #[test]
    fn flags_duplicate_function() {
        assert_eq!(run_on("function foo() {} function foo() {}").len(), 1);
    }

    #[test]
    fn flags_duplicate_let_in_block() {
        assert_eq!(run_on("{ let x = 1; let x = 2; }").len(), 1);
    }

    #[test]
    fn allows_same_name_in_different_scopes() {
        assert!(run_on("function a() {} function b() { var a = 1; }").is_empty());
    }

    #[test]
    fn allows_single_declaration() {
        assert!(run_on("var a = 1; const b = 2;").is_empty());
    }
}
