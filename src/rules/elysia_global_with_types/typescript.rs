//! elysia-global-with-types backend — flag global-scoped plugins that expose typed context.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    // Cheap textual gate: must contain a global scope marker AND a typed-state method.
    let has_global = ctx.source_contains("as:'global'")
        || ctx.source_contains("as: 'global'")
        || ctx.source_contains("as:\"global\"")
        || ctx.source_contains("as: \"global\"")
        || ctx.source_contains(".as('global')")
        || ctx.source_contains(".as(\"global\")");
    if !has_global {
        return;
    }
    let has_typed = ctx.source_contains(".state(") || ctx.source_contains(".decorate(") || ctx.source_contains(".model(");
    if !has_typed {
        return;
    }

    // Only emit once — anchor on the first `.state(`, `.decorate(`, or `.model(` call.
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let prop_text = property.utf8_text(source).unwrap_or("");
    if prop_text != "state" && prop_text != "decorate" && prop_text != "model" {
        return;
    }

    // Avoid duplicates: only flag if no diagnostic for this rule has been pushed yet.
    if diagnostics.iter().any(|d| d.rule_id == "elysia-global-with-types") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-global-with-types".into(),
        message: "Global-scoped plugin exposes typed context (`state`/`decorate`/`model`) — types leak into every consumer. Use `as: 'scoped'`.".into(),
        severity: Severity::Warning,
        span: None,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_global_with_state() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().state('x', 1).onBeforeHandle({ as: 'global' }, () => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_global_with_decorate() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().decorate('foo', 1).as('global');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_scoped_with_state() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().state('x', 1).as('scoped');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_global_without_typed_state() {
        let src = "import { Elysia } from 'elysia';\nexport const p = new Elysia().onBeforeHandle({ as: 'global' }, () => {});";
        assert!(run_on(src).is_empty());
    }
}
