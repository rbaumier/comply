//! Flag `new Pool(...)` / `drizzle(...)` when they are inside any function
//! body (function_declaration, arrow_function, function_expression,
//! method_definition).

use crate::diagnostic::{Diagnostic, Severity};

const FUNC_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "arrow_function",
    "method_definition",
    "function",
];

fn inside_function(node: tree_sitter::Node<'_>) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if FUNC_KINDS.contains(&parent.kind()) {
            return true;
        }
        cur = parent;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // new Pool(...)
    if node.kind() == "new_expression" {
        let Some(ctor) = node.child_by_field_name("constructor") else { return };
        if ctor.utf8_text(source).unwrap_or("") != "Pool" {
            return;
        }
        if !inside_function(node) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`new Pool()` in a handler body — move to module scope so connections are reused across requests.".into(),
            Severity::Warning,
        ));
        return;
    }
    // drizzle(...)
    if node.kind() == "call_expression" {
        let Some(func) = node.child_by_field_name("function") else { return };
        if func.kind() != "identifier" {
            return;
        }
        if func.utf8_text(source).unwrap_or("") != "drizzle" {
            return;
        }
        if !inside_function(node) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`drizzle()` in a handler body — move to module scope so the client is reused across requests.".into(),
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
    fn flags_new_pool_in_handler() {
        let src = "export async function handler() { const pool = new Pool({}); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_drizzle_in_handler() {
        let src = "export const handler = async () => { const db = drizzle(pool); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_module_scope_pool() {
        let src = "const pool = new Pool({});\nconst db = drizzle(pool);";
        assert!(run(src).is_empty());
    }
}
