//! prefer-expect-resolves — flag `expect(await promise)` calls.
//!
//! Detects `call_expression` nodes whose callee is the identifier `expect`
//! and whose sole argument is an `await_expression`. In that case, the
//! assertion can be rewritten as `await expect(promise).resolves.<matcher>`
//! which surfaces rejection-related failures as matcher failures rather
//! than uncaught promise rejections.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "expect" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // arguments is the `arguments` node (parenthesized list). Collect named children.
    let mut named = Vec::new();
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        named.push(child);
    }
    if named.len() != 1 {
        return;
    }
    let arg = named[0];
    if arg.kind() != "await_expression" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-expect-resolves".into(),
        message: "Use `await expect(promise).resolves` instead of `expect(await promise)`.".into(),
        severity: Severity::Warning,
        span: Some((range.start, range.end - range.start)),
    });
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
    
    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_expect_await_promise() {
        let d = run_on("async function t() { expect(await fetchUser()).toEqual({id: 1}); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-expect-resolves");
    }

    #[test]
    fn flags_expect_await_identifier() {
        let d = run_on("async function t() { expect(await p).toBe(42); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_await_expect_resolves() {
        let d =
            run_on("async function t() { await expect(fetchUser()).resolves.toEqual({id: 1}); }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_expect_on_sync_value() {
        let d = run_on("function t() { expect(1 + 1).toBe(2); }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_expect_on_promise_without_await() {
        let d = run_on("function t() { expect(fetchUser()).toBeInstanceOf(Promise); }");
        assert!(d.is_empty());
    }
}
