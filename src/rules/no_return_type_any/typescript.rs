//! no-return-type-any backend — functions with explicit `: any` return type.
//!
//! Walks function-shaped nodes (`function_declaration`, `function_expression`,
//! `arrow_function`, `method_definition`, `method_signature`,
//! `abstract_method_signature`) and inspects the direct child `type_annotation`
//! that follows the parameter list. The annotation is flagged when its inner
//! type resolves to `any` directly or as `Promise<any>`.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// True if the given type-bearing node is `any` or `Promise<any>` (top-level
/// type argument).
fn resolves_to_any(node: Node, source: &[u8]) -> bool {
    match node.kind() {
        "predefined_type" => std::str::from_utf8(&source[node.byte_range()]).unwrap_or("") == "any",
        "generic_type" => {
            let Some(name) = node.child_by_field_name("name") else {
                return false;
            };
            if std::str::from_utf8(&source[name.byte_range()]).unwrap_or("") != "Promise" {
                return false;
            }
            let Some(args) = node.child_by_field_name("type_arguments") else {
                return false;
            };
            let mut cursor = args.walk();
            args.named_children(&mut cursor)
                .any(|c| resolves_to_any(c, source))
        }
        _ => false,
    }
}

/// Find the return-type `type_annotation` that is a direct child of a
/// function-like node, sitting between the parameter list and the body.
fn return_type_annotation<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "type_annotation")
}

crate::ast_check! { on ["function_declaration", "function_expression", "arrow_function", "method_definition", "method_signature", "abstract_method_signature"] => |node, source, ctx, diagnostics|
    let Some(type_ann) = return_type_annotation(node) else { return };
    let mut cursor = type_ann.walk();
    let Some(inner) = type_ann.named_children(&mut cursor).next() else { return };

    if !resolves_to_any(inner, source) {
        return;
    }

    let pos = type_ann.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-return-type-any".into(),
        message: "Function has explicit `: any` return type — use a specific type or `unknown`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_any_return_function() {
        assert_eq!(run_on("function foo(): any {").len(), 1);
    }

    #[test]
    fn flags_any_return_arrow() {
        assert_eq!(run_on("const foo = (): any => {};").len(), 1);
    }

    #[test]
    fn flags_promise_any_return() {
        assert_eq!(run_on("async function foo(): Promise<any> {").len(), 1);
    }

    #[test]
    fn allows_specific_return_type() {
        assert!(run_on("function foo(): string {").is_empty());
    }

    #[test]
    fn allows_unknown_return() {
        assert!(run_on("function foo(): unknown {").is_empty());
    }
}
