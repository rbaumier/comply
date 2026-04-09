//! no-inline-param-type backend — reject inline object types in function
//! parameters.
//!
//! Why: `function f(opts: { name: string; age: number }) {}` names the
//! parameter shape ad-hoc, with no way to reference or reuse it. When the
//! same shape appears in a second function, the author copies the type
//! literal instead of extracting a named type — now two definitions can
//! drift. A named type (`type UserOptions = { ... }`) gives the shape an
//! identity and a single place to maintain.
//!
//! Detection: walk `required_parameter` / `optional_parameter` nodes
//! whose `type_annotation` contains an `object_type` literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "required_parameter" && node.kind() != "optional_parameter" {
                return;
            }
            if !has_inline_object_type(node) {
                return;
            }
            let name = param_name(node, source_bytes).unwrap_or("<param>");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-inline-param-type".into(),
                message: format!(
                    "Parameter '{name}' has an inline object type — extract \
                     it to a named `type` declaration above the function so \
                     the shape has an identity and can't silently drift."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn has_inline_object_type(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "type_annotation" {
            continue;
        }
        let mut ta_cursor = child.walk();
        if child
            .children(&mut ta_cursor)
            .any(|c| c.kind() == "object_type")
        {
            return true;
        }
    }
    false
}

fn param_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
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
            &CheckCtx {
                path: Path::new("t.ts"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_inline_object_param() {
        assert_eq!(
            run_on("function f(opts: { name: string; age: number }) {}").len(),
            1
        );
    }

    #[test]
    fn allows_named_type_param() {
        assert!(run_on("function f(opts: UserOptions) {}").is_empty());
    }

    #[test]
    fn allows_primitive_type_param() {
        assert!(run_on("function f(name: string) {}").is_empty());
    }

    #[test]
    fn flags_inline_on_arrow_function() {
        assert_eq!(run_on("const f = (opts: { a: number }) => opts.a;").len(), 1);
    }
}
