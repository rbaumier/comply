//! tanstack-query-fn-must-throw-on-error backend.
//!
//! Walk `queryFn` property values; when the body calls `fetch(...)` but
//! never accesses `.ok` on the response, flag it. TanStack Query relies on
//! a thrown error to retry and surface failures, so a `queryFn` that
//! returns `res.json()` directly silently swallows HTTP errors.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] prefilter = ["queryFn"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "queryFn" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if !subtree_calls(value, source, "fetch") { return; }
    if subtree_has_member(value, source, "ok") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`queryFn` with `fetch()` must check `res.ok` and throw on error so TanStack Query can retry.".into(),
        Severity::Warning,
    ));
}

/// True if any descendant of `root` is a `call_expression` whose callee is
/// either the bare identifier `name` or a member expression ending in `name`.
fn subtree_calls(root: tree_sitter::Node<'_>, source: &[u8], name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
        {
            let target = match func.kind() {
                "identifier" => Some(func),
                "member_expression" => func.child_by_field_name("property"),
                _ => None,
            };
            if let Some(t) = target
                && let Ok(text) = t.utf8_text(source)
                && text == name
            {
                return true;
            }
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if any descendant of `root` is a `member_expression` whose property
/// is `name` (e.g. `res.ok`, `response.ok`).
fn subtree_has_member(root: tree_sitter::Node<'_>, source: &[u8], name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "member_expression"
            && let Some(prop) = n.child_by_field_name("property")
            && let Ok(text) = prop.utf8_text(source)
            && text == name
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
    fn flags_fetch_no_ok_check() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: async () => { const res = await fetch('/api'); return res.json() } })")
                .len(),
            1
        );
    }

    #[test]
    fn allows_with_ok_check() {
        assert!(run(
            "useQuery({ queryKey: ['x'], queryFn: async () => { const res = await fetch('/api'); if (!res.ok) throw new Error('err'); return res.json() } })"
        )
        .is_empty());
    }
}
