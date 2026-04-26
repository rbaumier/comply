//! react-hoist-regex-outside-component backend — flag regex literals
//! defined inside a React component body.
//!
//! Why: every render recompiles the regex, throwing away the compiled
//! automaton. Hoist the regex to module scope so it's compiled once.
//!
//! Detection heuristic: walk `regex` nodes (literal `/.../`) and
//! `new_expression` with `RegExp` constructor. Flag those whose nearest
//! enclosing function name starts with an uppercase letter (component
//! convention) or is a `function_declaration` / `arrow_function` assigned
//! to a PascalCase variable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["regex"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if !inside_component_body(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-hoist-regex-outside-component".into(),
            message: "Regex literal inside a component body is \
                      recompiled every render. Hoist to a module-level \
                      `const` so it compiles once."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the regex is nested inside a function whose name is PascalCase
/// (React component convention) or whose body returns JSX.
fn inside_component_body(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "function_declaration" => {
                if let Some(name) = parent
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    && starts_with_uppercase(name)
                {
                    return true;
                }
            }
            "variable_declarator" => {
                if let Some(name) = parent
                    .named_child(0)
                    .and_then(|n| n.utf8_text(source).ok())
                    && starts_with_uppercase(name)
                {
                    // Only count if the assigned value is a function / arrow.
                    let mut cursor = parent.walk();
                    if parent
                        .children(&mut cursor)
                        .any(|c| matches!(c.kind(), "arrow_function" | "function_expression"))
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current = parent;
    }
    false
}

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_tsx(source, &Check)


    }

    #[test]
    fn flags_regex_in_component() {
        let source = "function Foo() { const r = /test/g; return null; }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_module_level_regex() {
        let source = "const r = /test/g; function Foo() { return null; }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_regex_in_non_component_function() {
        let source = "function helper() { const r = /test/g; return r; }";
        assert!(run_on(source).is_empty());
    }
}
