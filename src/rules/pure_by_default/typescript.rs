//! pure-by-default backend — flag functions referencing top-level mutable state.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a function body references a given variable name (whole-word match).
fn body_references_var(body: tree_sitter::Node, source: &[u8], var_name: &str) -> bool {
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier"
            && let Ok(text) = n.utf8_text(source)
                && text == var_name {
                    return true;
                }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only process at the program level to collect mutable vars and functions
    if node.kind() != "program" {
        return;
    }

    // 1. Collect top-level `let`/`var` variable names
    let mut mutable_vars: Vec<String> = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "lexical_declaration" || child.kind() == "variable_declaration" {
            let Ok(text) = child.utf8_text(source) else { continue };
            // lexical_declaration starts with `let`/`const`/`var`
            if text.starts_with("let ") || text.starts_with("var ") {
                // Extract variable name(s) from declarators
                let mut dc = child.walk();
                for decl in child.children(&mut dc) {
                    if decl.kind() == "variable_declarator"
                        && let Some(name_node) = decl.child_by_field_name("name")
                            && let Ok(name) = name_node.utf8_text(source) {
                                mutable_vars.push(name.to_string());
                            }
                }
            }
        }
    }

    if mutable_vars.is_empty() {
        return;
    }

    // 2. Find top-level function declarations and check if they reference mutable vars
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        let func_node = if child.kind() == "function_declaration" {
            child
        } else if child.kind() == "export_statement" {
            let mut ec = child.walk();
            match child.children(&mut ec).find(|c| c.kind() == "function_declaration") {
                Some(fd) => fd,
                None => continue,
            }
        } else {
            continue;
        };

        let Some(name_node) = func_node.child_by_field_name("name") else { continue };
        let Ok(func_name) = name_node.utf8_text(source) else { continue };
        let Some(body) = func_node.child_by_field_name("body") else { continue };

        for var in &mutable_vars {
            if body_references_var(body, source, var) {
                let pos = func_node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "pure-by-default".into(),
                    message: format!(
                        "Function `{}` references mutable top-level state `{}`.",
                        func_name, var,
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                break; // one diagnostic per function is enough
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
    fn flags_function_using_top_level_let() {
        let src = "\
let counter = 0;

function increment() {
    counter += 1;
}
";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn allows_function_without_top_level_state() {
        let src = "\
const MAX = 100;

function add(a: number, b: number) {
    return a + b;
}
";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_var_at_top_level() {
        let src = "\
var state = {};

function reset() {
    state = {};
}
";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reset"));
    }

    #[test]
    fn ignores_let_inside_function() {
        let src = "\
function foo() {
    let x = 1;
    return x;
}
";
        assert!(run_on(src).is_empty());
    }
}
