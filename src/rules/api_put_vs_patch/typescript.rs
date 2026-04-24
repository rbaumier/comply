//! Detect `.put(...)` route registrations whose handler signature
//! (arrow / function) mentions `Partial<...>` in a type annotation.
//!
//! Shape scanned:
//! ```ts
//! app.put("/users/:id", (req: Request<..., ..., Partial<User>>, res) => { ... })
//! router.put("/x", handler: (body: Partial<X>) => ...)
//! ```
//!
//! The detection is intentionally syntactic: we match a call expression
//! whose callee is `<something>.put` and look for `Partial<...>` tokens
//! anywhere in the call's argument subtree.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `callee` is a member expression `<x>.put` (case-sensitive).
fn is_put_member(callee: tree_sitter::Node, source: &[u8]) -> bool {
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    let Ok(name) = std::str::from_utf8(&source[prop.byte_range()]) else {
        return false;
    };
    name == "put"
}

/// Walk `root` looking for a `generic_type` whose name is `Partial`.
fn contains_partial(root: tree_sitter::Node, source: &[u8]) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "generic_type"
            && let Some(name_node) = n.child_by_field_name("name")
                && let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()])
                    && name == "Partial" {
                        return true;
                    }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !is_put_member(callee, source) { return }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if !contains_partial(args, source) { return }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "PUT handler accepts `Partial<...>` — use `.patch(...)` for partial updates so clients keep idempotency guarantees for PUT.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_put_with_partial_in_handler() {
        let d = run(
            "app.put('/users/:id', (req: Request<{id: string}, {}, Partial<User>>, res) => {});",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_put_with_partial_in_body_type() {
        let d = run("router.put('/x', (body: Partial<Thing>) => body);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_put_with_full_type() {
        assert!(run("app.put('/users/:id', (req: Request<{id: string}, {}, User>, res) => {});").is_empty());
    }

    #[test]
    fn allows_patch_with_partial() {
        assert!(run("app.patch('/users/:id', (req: Request<{}, {}, Partial<User>>, res) => {});").is_empty());
    }

    #[test]
    fn allows_non_route_put_method() {
        // `.put(...)` on a map-like is ignored unless Partial appears — no false positive here.
        assert!(run("const m = new Map(); m.put('k', 'v');").is_empty());
    }
}
