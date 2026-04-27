//! elysia-scope-missing backend — flag plugin lifecycle hooks without a scope.

use crate::diagnostic::{Diagnostic, Severity};

const HOOK_METHODS: &[&str] = &["onBeforeHandle", "onAfterHandle", "onError", "onRequest", "onTransform"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source.contains("export") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let prop_text = property.utf8_text(source).unwrap_or("");
    if !HOOK_METHODS.contains(&prop_text) {
        return;
    }

    // If the file uses any scope marker, skip — fuzzy but cheap.
    let s = ctx.source;
    let has_scope = s.contains("as:'global'")
        || s.contains("as: 'global'")
        || s.contains("as:\"global\"")
        || s.contains("as: \"global\"")
        || s.contains("as:'scoped'")
        || s.contains("as: 'scoped'")
        || s.contains("as:\"scoped\"")
        || s.contains("as: \"scoped\"")
        || s.contains(".as('scoped')")
        || s.contains(".as(\"scoped\")")
        || s.contains(".as('global')")
        || s.contains(".as(\"global\")");
    if has_scope {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-scope-missing".into(),
        message: format!(
            "`{}` in an exported plugin without a scope — hooks default to `local` and won't propagate to the parent app.",
            prop_text
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_hook_without_scope() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia().onBeforeHandle(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_onerror_without_scope() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia().onError(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_scoped_hook() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia().onBeforeHandle({ as: 'global' }, () => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_as_scoped_call() {
        let src = "import { Elysia } from 'elysia';\nexport const plugin = new Elysia().onBeforeHandle(() => {}).as('scoped');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_exported_app() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().onBeforeHandle(() => {});";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
