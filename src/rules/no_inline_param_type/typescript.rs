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

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["required_parameter", "optional_parameter"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if !has_inline_object_type(node) {
            return;
        }
        if is_destructured_param(node) {
            return;
        }
        if is_react_component_param(node, source_bytes) {
            return;
        }
        let name = param_name(node, source_bytes).unwrap_or("<param>");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-inline-param-type".into(),
            message: format!(
                "Parameter '{name}' has an inline object type — extract \
                 it to a named `type` declaration above the function so \
                 the shape has an identity and can't silently drift."
            ),
            severity: Severity::Warning,
            span: None,
        });
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

fn is_destructured_param(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "object_pattern")
}

/// True when the parameter belongs to a function whose name starts with an
/// uppercase letter (React component convention).
fn is_react_component_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "function_declaration" | "function" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source) {
                        return name.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase());
                    }
                }
                return false;
            }
            "variable_declarator" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source) {
                        return name.as_bytes().first().is_some_and(|b| b.is_ascii_uppercase());
                    }
                }
                return false;
            }
            _ => current = parent,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
        assert_eq!(
            run_on("const f = (opts: { a: number }) => opts.a;").len(),
            1
        );
    }

    #[test]
    fn allows_react_component_inline_props() {
        assert!(run_on("function UserCard({ name }: { name: string }) {}").is_empty());
    }

    #[test]
    fn allows_react_arrow_component_inline_props() {
        assert!(run_on("const UserCard = ({ name }: { name: string }) => null;").is_empty());
    }

    #[test]
    fn allows_destructured_param() {
        assert!(run_on("function createPlugin({ db, auth }: { db: Database; auth: Auth }) {}").is_empty());
    }
}
