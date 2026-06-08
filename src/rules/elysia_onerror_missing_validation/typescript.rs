//! elysia-onerror-missing-validation backend — flag onError handlers that don't handle VALIDATION.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["\"onError\""] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    if property.utf8_text(source).unwrap_or("") != "onError" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    if args_text.contains("VALIDATION") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-onerror-missing-validation".into(),
        message: "`onError` handler doesn't branch on `'VALIDATION'` — schema errors will surface as generic 500s.".into(),
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
    fn flags_onerror_without_validation() {
        let src = "import { Elysia } from 'elysia';\napp.onError(({ error }) => 'oops');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_onerror_with_other_codes() {
        let src = "import { Elysia } from 'elysia';\napp.onError(({ code }) => code === 'NOT_FOUND' ? 'nf' : 'oops');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_onerror_with_validation_branch() {
        let src = "import { Elysia } from 'elysia';\napp.onError(({ code, error }) => code === 'VALIDATION' ? error.message : 'oops');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onError(() => 'oops');";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn ignores_use_mutation_on_error_object_property() {
        // Regression for #202: `useMutation({ onError: ... })` is a TanStack
        // Query callback, not an Elysia `.onError()` member call.
        let src = "import { useMutation } from '@tanstack/react-query';\n\
            useMutation({ onError: (error, variables, context, mutation) => { console.log(error); } });";
        assert!(run_on(src).is_empty());
    }
}
