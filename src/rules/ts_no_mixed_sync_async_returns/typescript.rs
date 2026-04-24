//! Flags `union_type`s in return-type position that include a
//! `Promise<...>` alternative alongside a non-Promise alternative.

use crate::diagnostic::{Diagnostic, Severity};

fn is_promise_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    // generic_type with name "Promise"
    if node.kind() == "generic_type"
        && let Some(name) = node.child_by_field_name("name")
    {
        let text = std::str::from_utf8(&source[name.byte_range()]).unwrap_or("");
        return text == "Promise";
    }
    false
}

fn is_return_type_position(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    // return_type_annotation or type_annotation as return type
    parent.kind() == "type_annotation" || parent.kind() == "return_type"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "union_type" {
        return;
    }

    // Walk up through parentheses / type_annotation to check we're in a return type.
    let mut probe = node;
    let mut in_return_type = false;
    for _ in 0..4 {
        let Some(parent) = probe.parent() else { break };
        let pk = parent.kind();
        if pk == "function_signature"
            || pk == "function_declaration"
            || pk == "method_signature"
            || pk == "method_definition"
            || pk == "arrow_function"
            || pk == "function_expression"
            || pk == "function_type"
        {
            // Only flag if we're the return_type child, not a parameter type.
            if let Some(ret) = parent.child_by_field_name("return_type")
                && ret.id() == probe.id()
            {
                in_return_type = true;
            }
            break;
        }
        if !is_return_type_position(probe) && pk != "parenthesized_type" {
            // continue walking up through parens/annotations
        }
        probe = parent;
    }

    if !in_return_type {
        return;
    }

    let mut cursor = node.walk();
    let members: Vec<_> = node.named_children(&mut cursor).collect();
    let has_promise = members.iter().any(|m| is_promise_type(*m, source));
    let has_non_promise = members.iter().any(|m| !is_promise_type(*m, source));

    if has_promise && has_non_promise {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Return type mixes sync and Promise values; mark the function `async` so it always returns a Promise.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_mixed_return_type() {
        let src = "function f(): string | Promise<string> { return 'x'; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_mixed_method_signature() {
        let src = "interface I { run(): number | Promise<number>; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_pure_promise_return() {
        let src = "function f(): Promise<string> { return Promise.resolve('x'); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_union_in_parameter() {
        let src = "function f(x: string | Promise<string>): void {}";
        assert!(run(src).is_empty());
    }
}
