//! elysia-scope-missing backend — flag **plugin** lifecycle hooks without a scope.
//!
//! Hooks on the root app are global by construction — only plugins need an
//! explicit `as: 'global' | 'scoped'`. Skip files that look like the root
//! app (call `.listen(...)`, or named `app` / `index` / `server` / `main` /
//! `create-app` / `bootstrap` / `entry`).

use crate::diagnostic::{Diagnostic, Severity};

const HOOK_METHODS: &[&str] = &[
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "onTransform",
];

fn is_root_app_file(source: &str, path: &std::path::Path) -> bool {
    if crate::oxc_helpers::source_contains(source, ".listen(") {
        return true;
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        stem,
        "app" | "index" | "server" | "main" | "create-app" | "createApp" | "bootstrap" | "entry"
    )
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source_contains("export") {
        return;
    }
    if is_root_app_file(ctx.source, ctx.path) {
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
    let has_scope = ctx.source_contains("as:'global'")
        || ctx.source_contains("as: 'global'")
        || ctx.source_contains("as:\"global\"")
        || ctx.source_contains("as: \"global\"")
        || ctx.source_contains("as:'scoped'")
        || ctx.source_contains("as: 'scoped'")
        || ctx.source_contains("as:\"scoped\"")
        || ctx.source_contains("as: \"scoped\"")
        || ctx.source_contains(".as('scoped')")
        || ctx.source_contains(".as(\"scoped\")")
        || ctx.source_contains(".as('global')")
        || ctx.source_contains(".as(\"global\")");
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
        let src =
            "import { Elysia } from 'elysia';\nconst app = new Elysia().onBeforeHandle(() => {});";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn ignores_root_app_with_listen() {
        // Regression: root app's hooks are global by construction; flagging
        // them is a false positive.
        let src = "import { Elysia } from 'elysia';\nexport const app = new Elysia().onError(() => {}).listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_use_mutation_on_error_object_property() {
        // Regression for #202: `useMutation({ onError: ... })` is a TanStack
        // Query callback, not an Elysia plugin hook member call.
        let src = "import { useMutation } from '@tanstack/react-query';\n\
            export const useFormMutation = () => useMutation({\n\
              onError: (error, variables, context, mutation) => { console.log(error); }\n\
            });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_root_app_in_create_app_file() {
        use crate::project::ProjectCtx;
                use std::path::Path;
        let project = ProjectCtx::for_test_with_framework("elysia");
        // Regression: `createApp()` returns the root app instance — calls to
        // `onError` / `onRequest` here aren't plugin hooks.
        let src = "import { Elysia } from 'elysia';\nexport const createApp = () => new Elysia().onError(() => {}).onRequest(() => {});";
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, src, Path::new("src/create-app.ts"), &project, crate::rules::file_ctx::default_static_file_ctx())
                .is_empty()
        );
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, src, Path::new("src/app.ts"), &project, crate::rules::file_ctx::default_static_file_ctx()).is_empty()
        );
        assert!(
            crate::rules::test_helpers::run_rule_with_ctx(&Check, src, Path::new("src/server.ts"), &project, crate::rules::file_ctx::default_static_file_ctx())
                .is_empty()
        );
    }
}
