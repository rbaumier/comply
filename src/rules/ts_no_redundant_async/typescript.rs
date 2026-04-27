//! Detect functions of the form:
//!
//! ```ignore
//! async function f() { return await expr; }
//! const f = async () => { return await expr; };
//! const f = async () => await expr;
//! ```
//!
//! and that contain no other awaits, no try blocks. Such a function is
//! semantically equivalent to dropping `async`/`await` entirely (modulo
//! micro-task timing nobody depends on).

use crate::diagnostic::{Diagnostic, Severity};

/// Walk a node (inclusive) and answer two questions:
/// - Does it contain a `try_statement`?
/// - How many `await_expression` nodes does it contain?
fn scan(node: tree_sitter::Node) -> (bool, usize) {
    let mut has_try = false;
    let mut awaits = 0usize;
    let mut stack: Vec<tree_sitter::Node> = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "try_statement" => has_try = true,
            "await_expression" => awaits += 1,
            // Don't recurse into nested function bodies.
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition" => continue,
            _ => {}
        }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
    (has_try, awaits)
}

/// Is this body either:
/// - a statement_block whose only meaningful statement is `return await X`, or
/// - an expression body that is itself `await X` (arrow shorthand)?
fn is_single_return_await(body: tree_sitter::Node) -> bool {
    if body.kind() == "statement_block" {
        // Find a single return_statement returning an await_expression.
        let mut found: Option<tree_sitter::Node> = None;
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "return_statement" {
                if found.is_some() {
                    return false;
                }
                found = Some(child);
            } else {
                // Any other named child means it's not a single return.
                return false;
            }
        }
        let Some(ret) = found else { return false };
        // The return's child should be an await_expression.
        let mut c = ret.walk();
        ret.named_children(&mut c).any(|n| n.kind() == "await_expression")
    } else if body.kind() == "await_expression" {
        // Arrow function with `async () => await x` body.
        true
    } else {
        false
    }
}

fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    // For an arrow_function, look at the first non-named child (the keyword).
    // Easier: walk children and look for an `async` keyword token.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "async" {
            return true;
        }
        // Stop scanning once we hit the parameters or body.
        if matches!(
            child.kind(),
            "formal_parameters" | "statement_block" | "=>" | "name" | "identifier"
        ) {
            break;
        }
    }
    // Fallback: substring scan of the leading text up to the body.
    let Ok(text) = node.utf8_text(source) else { return false };
    let head = text.split("=>").next().unwrap_or(text);
    head.contains("async")
}

crate::ast_check! {
    on ["function_declaration", "function_expression", "arrow_function", "method_definition"]
    => |node, source, ctx, diagnostics|
    if !is_async_function(node, source) { return; }
    let Some(body) = node.child_by_field_name("body") else { return; };
    if !is_single_return_await(body) { return; }
    let (has_try, awaits) = scan(body);
    if has_try { return; }
    if awaits != 1 { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Redundant `async`/`await`: this function only does `return await expr` \
                  with no try/catch — drop `async` and `await` and return the promise directly."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_return_await_in_block() {
        let src = "async function f() { return await fetch(url); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_arrow_return_await() {
        let src = "const f = async () => { return await fetch(url); };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_arrow_expression_await() {
        let src = "const f = async () => await fetch(url);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_try_catch() {
        let src = "async function f() { try { return await fetch(url); } catch (e) { return null; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_multiple_awaits() {
        let src = "async function f() { const a = await one(); return await two(a); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_async_function() {
        let src = "function f() { return fetch(url); }";
        assert!(run(src).is_empty());
    }
}
