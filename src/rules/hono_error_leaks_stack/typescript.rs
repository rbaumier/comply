//! Flag `err.stack` / `err.message` reads inside `app.onError(...)` callbacks.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk up the AST looking for a call_expression whose callee text ends with `.onError`.
fn inside_on_error(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression" {
            if let Some(callee) = parent.child_by_field_name("function") {
                let text = std::str::from_utf8(&source[callee.byte_range()]).unwrap_or("");
                if text.ends_with(".onError") || text == "onError" {
                    return Some(text.to_string());
                }
            }
        }
        cur = parent;
    }
    None
}

crate::ast_check! { on ["member_expression"] prefilter = ["hono", "Hono"] => |node, source, ctx, diagnostics|
    if !ctx.source_contains("hono") && !ctx.source_contains("Hono") { return; }
    let text = node.utf8_text(source).unwrap_or("");
    // Match `<ident>.stack` or `<ident>.message`.
    let property = node.child_by_field_name("property");
    let Some(property) = property else { return; };
    let prop = property.utf8_text(source).unwrap_or("");
    if prop != "stack" && prop != "message" { return; }

    // Object should look like the error parameter (single identifier, often `err`/`error`/`e`).
    let Some(object) = node.child_by_field_name("object") else { return; };
    if object.kind() != "identifier" { return; }
    let obj_name = object.utf8_text(source).unwrap_or("");
    if !matches!(obj_name, "err" | "error" | "e" | "exception") { return; }

    // Must be inside an onError callback.
    if inside_on_error(node, source).is_none() { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Returning `{text}` from `onError` leaks internal error details to clients."),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_err_stack() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ stack: err.stack }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_err_message() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ error: err.message }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_both() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ error: err.message, stack: err.stack }));";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_generic_message() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.onError((err, c) => c.json({ error: 'Internal Server Error' }, 500));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_outside_on_error() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\nfunction h(err: Error) { return err.message; }";
        assert!(run(src).is_empty());
    }
}
