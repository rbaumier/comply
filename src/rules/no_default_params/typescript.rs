//! no-default-params backend — flag any function parameter with a default.
//!
//! Default parameters hide behavior: `createUser(name, role = 'viewer')`
//! means the caller doesn't have to think about the role. Explicit factory
//! methods (`createViewer(name)` / `createAdmin(name)`) make every code
//! path self-documenting and independently testable.
//!
//! Detection: walk `required_parameter` nodes and flag any that contain
//! an `=` child — that's the TS grammar's way of representing a default value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "required_parameter" && node.kind() != "optional_parameter" {
                return;
            }
            if !has_default_value(node) {
                return;
            }
            let param_name = extract_param_name(node, source_bytes).unwrap_or("<param>");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-default-params".into(),
                message: format!(
                    "Parameter '{param_name}' has a default value — extract \
                     an explicit factory method instead. Default params hide \
                     behavior and create invisible coupling."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

/// True if the parameter node has an `=` child (the default-value marker).
fn has_default_value(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "=" {
            return true;
        }
    }
    false
}

/// Extract the parameter's identifier name for the diagnostic message.
fn extract_param_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_function_with_default_param() {
        let diags = run_on("function f(x = 5) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'x'"));
    }

    #[test]
    fn flags_typed_param_with_default() {
        let diags = run_on("function f(role: string = 'viewer') {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'role'"));
    }

    #[test]
    fn allows_param_without_default() {
        assert!(run_on("function f(x: number, y: string) {}").is_empty());
    }

    #[test]
    fn flags_arrow_function_default() {
        let diags = run_on("const f = (x = 5) => x;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_multiple_defaults() {
        assert_eq!(run_on("function f(a = 1, b: number, c = 3) {}").len(), 2);
    }
}
