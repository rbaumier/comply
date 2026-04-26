//! Detects the narrowing-lost-across-closure smell:
//!
//! ```ts
//! if (user) {
//!   setTimeout(() => console.log(user.name), 0); // `user` widens back to T | undefined
//! }
//! ```
//!
//! Heuristic: inside an `if (x)` / `if (x !== null)` / `if (typeof x === ...)`
//! block, if a call to `setTimeout`/`.then`/`.catch`/`addEventListener` takes
//! a function expression that references `x` directly (not captured by a
//! local const), flag it.

use crate::diagnostic::{Diagnostic, Severity};

fn narrowed_identifier<'a>(cond: tree_sitter::Node<'a>, source: &[u8]) -> Option<String> {
    // if (x) — cond is an identifier
    if cond.kind() == "identifier" {
        return std::str::from_utf8(&source[cond.byte_range()]).ok().map(str::to_string);
    }
    // if (x !== null) — binary_expression with identifier on left
    if cond.kind() == "binary_expression"
        && let Some(left) = cond.child_by_field_name("left")
        && left.kind() == "identifier"
    {
        return std::str::from_utf8(&source[left.byte_range()]).ok().map(str::to_string);
    }
    None
}

fn is_closure_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else { return false };
    let text = std::str::from_utf8(&source[func.byte_range()]).unwrap_or("");
    text == "setTimeout"
        || text == "setInterval"
        || text.ends_with(".then")
        || text.ends_with(".catch")
        || text.ends_with(".finally")
        || text.ends_with(".addEventListener")
        || text == "queueMicrotask"
        || text == "requestAnimationFrame"
}

fn references_identifier(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() == "identifier" {
        let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
        return text == name;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if references_identifier(child, source, name) {
            return true;
        }
    }
    false
}

fn has_local_const_with_name(block: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut cursor = block.walk();
    for child in block.children(&mut cursor) {
        if child.kind() == "lexical_declaration" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if text.starts_with("const ") && text.contains(&format!("{name} =")) {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    let Some(cond) = node.child_by_field_name("condition") else { return };
    // Unwrap parenthesized_expression
    let inner_cond = cond.named_child(0).unwrap_or(cond);
    let Some(name) = narrowed_identifier(inner_cond, source) else { return };

    let Some(body) = node.child_by_field_name("consequence") else { return };

    // Don't flag if the block shadows `name` with a local const.
    if has_local_const_with_name(body, source, &name) {
        return;
    }

    // Walk the body looking for closure calls that reference `name`.
    fn visit(
        node: tree_sitter::Node,
        source: &[u8],
        name: &str,
        ctx: &crate::rules::backend::CheckCtx,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if node.kind() == "call_expression" && is_closure_call(node, source)
            && let Some(args) = node.child_by_field_name("arguments") {
                let mut cur = args.walk();
                for arg in args.named_children(&mut cur) {
                    if (arg.kind() == "arrow_function" || arg.kind() == "function_expression")
                        && let Some(body_node) = arg.child_by_field_name("body")
                        && references_identifier(body_node, source, name)
                    {
                        diagnostics.push(Diagnostic::at_node(
                            ctx.path,
                            &arg,
                            super::META.id,
                            format!("Variable `{name}` loses its narrowing inside this callback; capture it in a local const first."),
                            Severity::Warning,
                        ));
                    }
                }
            }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, name, ctx, diagnostics);
        }
    }

    visit(body, source, &name, ctx, diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_set_timeout_using_narrowed_var() {
        let src = "function f(user: { name: string } | null) { if (user) { setTimeout(() => console.log(user.name), 0); } }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_promise_then() {
        let src = "function f(u: string | null) { if (u) { p.then(() => console.log(u)); } }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_local_const_capture() {
        let src = "function f(user: { name: string } | null) { if (user) { const u = user; setTimeout(() => console.log(u.name), 0); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_usage_without_closure() {
        let src = "function f(user: { name: string } | null) { if (user) { console.log(user.name); } }";
        assert!(run(src).is_empty());
    }
}
