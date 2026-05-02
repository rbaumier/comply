//! no-function-overloads backend — reject ambient function overload signatures.
//!
//! Why: TypeScript's overload signatures don't enforce implementation
//! correctness — they're purely ambient declarations, and the compiler
//! checks the implementation against the LAST signature only. In practice,
//! overloads confuse callers, break inference, and hide bugs. Prefer union
//! parameter types or generic signatures that actually constrain the
//! implementation.
//!
//! Detection: walk the program's top-level children, group `function_signature`
//! nodes by their identifier name, and flag every signature that's part of
//! an overload group (2+ signatures with the same name).

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let root = tree.root_node();

        let mut signatures: HashMap<String, Vec<tree_sitter::Node>> = HashMap::new();
        let mut implementations: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            let (kind, inner) = match child.kind() {
                "function_signature" | "function_declaration" => (child.kind(), child),
                "export_statement" => {
                    let mut ec = child.walk();
                    let found = child.children(&mut ec).find(|c| {
                        c.kind() == "function_signature" || c.kind() == "function_declaration"
                    });
                    match found {
                        Some(n) => (n.kind(), n),
                        None => continue,
                    }
                }
                _ => continue,
            };
            let Some(name) = inner
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
            else {
                continue;
            };
            match kind {
                "function_signature" => {
                    signatures.entry(name.to_string()).or_default().push(inner);
                }
                "function_declaration" => {
                    implementations.insert(name.to_string());
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();
        for (name, nodes) in signatures {
            if nodes.len() < 2 {
                continue;
            }
            if implementations.contains(&name) {
                continue;
            }
            for node in nodes {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-function-overloads".into(),
                    message: format!(
                        "Function '{name}' has overload signatures — overloads \
                         don't constrain the implementation and break inference. \
                         Use a union parameter type or a generic signature instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_overloads_with_implementation() {
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_ambient_overloads_without_implementation() {
        let source = "
function foo(x: number): string;
function foo(x: string): number;
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_single_signature() {
        assert!(run_on("function foo(x: number): string { return String(x); }").is_empty());
    }

    #[test]
    fn allows_distinct_functions() {
        let source = "function foo(): void {} function bar(): void {}";
        assert!(run_on(source).is_empty());
    }
}
