//! api-no-array-root-response — flag `Response.json([...])`,
//! `res.json([...])`, `c.json([...])`, and `return json([...])` calls.
//!
//! AST detection: walk `call_expression` nodes whose callee is one of
//! `Response.json`, `res.json`, `c.json`, or bare `json` (when invoked
//! at the start of a return statement). Flag the call when its first
//! argument is an `array` literal.

use crate::diagnostic::{Diagnostic, Severity};

fn callee_text<'a>(call: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    call.child_by_field_name("function")
        .and_then(|f| f.utf8_text(source).ok())
        .unwrap_or("")
}

fn first_arg_is_array(call: tree_sitter::Node) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .next()
        .is_some_and(|n| n.kind() == "array")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let callee = callee_text(node, source);
    let is_method_call =
        callee == "Response.json" || callee == "res.json" || callee == "c.json";
    let is_bare_json = callee == "json"
        && node
            .parent()
            .is_some_and(|p| p.kind() == "return_statement");

    if !is_method_call && !is_bare_json {
        return;
    }
    if !first_arg_is_array(node) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Return `{ data: [...] }` instead of a root-level array — arrays can't be extended without breaking clients.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_response_json_array() {
        assert_eq!(
            run("export async function GET() { return Response.json([...users]) }").len(),
            1
        );
    }

    #[test]
    fn allows_object_response() {
        assert!(
            run("export async function GET() { return Response.json({ data: users }) }").is_empty()
        );
    }
}
