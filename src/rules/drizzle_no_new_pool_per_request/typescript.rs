//! Flag `new Pool(...)` / `drizzle(...)` when they sit inside an
//! *exported* function body — the canonical "new pool per request"
//! shape is a route handler exported from a module. Internal/factory
//! helpers and module-scope code are not flagged.

use crate::diagnostic::{Diagnostic, Severity};

const FUNC_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "arrow_function",
    "method_definition",
    "function",
];

/// True when `node` sits inside a function body whose declaration is
/// exported (`export function`, `export const x = (…) => …`, methods of
/// an `export class`).
fn inside_exported_function(node: tree_sitter::Node<'_>) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if FUNC_KINDS.contains(&parent.kind()) && function_decl_is_exported(parent) {
            return true;
        }
        cur = parent;
    }
    false
}

fn function_decl_is_exported(func: tree_sitter::Node<'_>) -> bool {
    match func.kind() {
        "function_declaration" => is_export_statement(func.parent()),
        "arrow_function" | "function_expression" | "function" => {
            let mut up = func.parent();
            while let Some(p) = up {
                match p.kind() {
                    "variable_declarator"
                    | "lexical_declaration"
                    | "variable_declaration" => up = p.parent(),
                    "export_statement" => return true,
                    _ => return false,
                }
            }
            false
        }
        "method_definition" => {
            let mut up = func.parent();
            while let Some(p) = up {
                match p.kind() {
                    "class_body" => up = p.parent(),
                    "class_declaration" => return is_export_statement(p.parent()),
                    _ => return false,
                }
            }
            false
        }
        _ => false,
    }
}

fn is_export_statement(node: Option<tree_sitter::Node<'_>>) -> bool {
    matches!(node.map(|n| n.kind()), Some("export_statement"))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // new Pool(...)
    if node.kind() == "new_expression" {
        let Some(ctor) = node.child_by_field_name("constructor") else { return };
        if ctor.utf8_text(source).unwrap_or("") != "Pool" {
            return;
        }
        if !inside_exported_function(node) {
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
        if !inside_exported_function(node) {
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

    #[test]
    fn allows_pool_in_internal_factory() {
        // REVIEW regression: a non-exported factory function is not a
        // request handler — it is intentionally creating a pool.
        let src = "function makePool() { const pool = new Pool({}); return pool; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_drizzle_in_internal_helper() {
        let src = "function makeDb(pool) { const db = drizzle(pool); return db; }";
        assert!(run(src).is_empty());
    }
}
