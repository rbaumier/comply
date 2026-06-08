//! elysia-hooks-before-routes backend — flag lifecycle hooks chained after routes.

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];
const HOOK_METHODS: &[&str] = &[
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "onTransform",
    "onParse",
    "onResponse",
];

/// Walk the chain `app.foo(...).bar(...).baz(...)` from the outermost call
/// down to the innermost, returning the sequence of method names in *call
/// order* (i.e. `[foo, bar, baz]`).
fn chain_methods<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Vec<(String, tree_sitter::Node<'a>)> {
    let mut out: Vec<(String, tree_sitter::Node<'a>)> = Vec::new();
    let mut cur = node;
    loop {
        if cur.kind() != "call_expression" {
            break;
        }
        let Some(callee) = cur.child_by_field_name("function") else {
            break;
        };
        if callee.kind() != "member_expression" {
            break;
        }
        let Some(property) = callee.child_by_field_name("property") else {
            break;
        };
        let prop = property.utf8_text(source).unwrap_or("").to_string();
        out.push((prop, cur));
        let Some(object) = callee.child_by_field_name("object") else {
            break;
        };
        cur = object;
    }
    out.reverse();
    out
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    // Only analyse the *outermost* call in a chain — skip if our parent is also
    // a member_expression whose object is us (i.e. we're an inner link).
    if let Some(parent) = node.parent() {
        if parent.kind() == "member_expression" {
            if let Some(obj) = parent.child_by_field_name("object") {
                if obj.id() == node.id() {
                    return;
                }
            }
        }
    }

    let methods = chain_methods(node, source);
    if methods.len() < 2 {
        return;
    }

    let mut seen_route = false;
    for (name, call_node) in &methods {
        if ROUTE_METHODS.contains(&name.as_str()) {
            seen_route = true;
            continue;
        }
        if seen_route && HOOK_METHODS.contains(&name.as_str()) {
            let pos = call_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "elysia-hooks-before-routes".into(),
                message: format!(
                    "`.{}(...)` chained after route definitions — Elysia hooks only apply to routes registered after them.",
                    name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_hook_after_route() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', () => 'ok').onBeforeHandle(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_onerror_after_post() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().post('/', () => 'ok').onError(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_hook_before_route() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onBeforeHandle(() => {}).get('/', () => 'ok');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "new Elysia().get('/', () => 'ok').onBeforeHandle(() => {});";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
