//! Detect `.put(...)` route registrations whose handler signature
//! mentions `Partial<...>` *as a type annotation*.
//!
//! Shape scanned:
//! ```ts
//! app.put("/users/:id", (req: Request<..., ..., Partial<User>>, res) => { ... })
//! router.put("/x", (body: Partial<X>) => ...)
//! ```
//!
//! The match is intentionally narrow: only `Partial<...>` appearing
//! inside a `type_annotation` (parameter / return-type slot) or inside
//! a `type_arguments` list (route-handler generic) on the put call's
//! arguments triggers the diagnostic. Bare textual mentions in strings
//! or values are ignored.

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

/// Walk `root`'s subtree but only descend into nodes that represent
/// type-level positions (parameter type annotations, return-type
/// annotations, type-argument lists). When a `generic_type` with name
/// `Partial` is reached inside such a position, return true.
fn contains_partial_in_type_position(root: tree_sitter::Node, source: &[u8]) -> bool {
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            // Type-level scopes — once inside, descend freely looking
            // for `Partial<...>`.
            "type_annotation" | "type_arguments" | "opting_type_annotation" => {
                if subtree_has_partial(n, source) {
                    return true;
                }
                continue;
            }
            _ => {}
        }
        // Otherwise keep looking for a type position deeper in the tree
        // (we don't dive into already-known leaf scopes here, to avoid
        // matching `Partial` inside string contents or value calls).
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn subtree_has_partial(root: tree_sitter::Node, source: &[u8]) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "generic_type"
            && let Some(name_node) = n.child_by_field_name("name")
            && let Ok(name) = std::str::from_utf8(&source[name_node.byte_range()])
            && name == "Partial"
        {
            return true;
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !is_put_member(callee, source) { return }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if !contains_partial_in_type_position(args, source) { return }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "PUT handler accepts `Partial<...>` — use `.patch(...)` for partial updates so clients keep idempotency guarantees for PUT.".into(),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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
        assert!(
            run("app.put('/users/:id', (req: Request<{id: string}, {}, User>, res) => {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_patch_with_partial() {
        assert!(
            run("app.patch('/users/:id', (req: Request<{}, {}, Partial<User>>, res) => {});")
                .is_empty()
        );
    }

    #[test]
    fn allows_non_route_put_method() {
        // `.put(...)` on a map-like is ignored unless Partial appears — no false positive here.
        assert!(run("const m = new Map(); m.put('k', 'v');").is_empty());
    }

    #[test]
    fn allows_partial_in_value_position_only() {
        // REVIEW regression: `Partial` referenced as a value (string or
        // local variable) must not trigger the rule — we only flag a
        // type-level `Partial<...>` in the handler signature.
        assert!(run("app.put('/x', (req, res) => { console.log('Partial<User>'); });").is_empty());
    }
}
