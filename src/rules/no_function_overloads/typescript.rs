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

        // Count function_signature nodes by name at program level.
        let mut counts: HashMap<String, Vec<tree_sitter::Node>> = HashMap::new();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            // `export_statement` wraps the actual declaration — peek inside.
            let signature = match child.kind() {
                "function_signature" => child,
                "export_statement" => {
                    let mut ec = child.walk();
                    let inner = child
                        .children(&mut ec)
                        .find(|c| c.kind() == "function_signature");
                    match inner {
                        Some(n) => n,
                        None => continue,
                    }
                }
                _ => continue,
            };
            let Some(name) = signature
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
            else {
                continue;
            };
            counts.entry(name.to_string()).or_default().push(signature);
        }

        let mut diagnostics = Vec::new();
        for (name, nodes) in counts {
            if nodes.len() < 2 {
                continue;
            }
            for node in nodes {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
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
    fn flags_overloaded_function() {
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
";
        // Two overload signatures → 2 diagnostics.
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
