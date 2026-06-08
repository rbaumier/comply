//! elysia-custom-errors-in-model backend — flag `class X extends Error`
//! declarations that live in a service file rather than a model file.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["class_declaration"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let path_str = ctx.path.to_string_lossy();
    if !path_str.contains("service") {
        return;
    }

    // Look for `extends Error` in the heritage clause.
    let mut found_error = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let txt = child.utf8_text(source).unwrap_or("");
            if txt.contains("extends Error") || txt.contains("extends\tError") {
                found_error = true;
            }
            break;
        }
    }
    if !found_error { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-custom-errors-in-model".into(),
        message: "Custom error class belongs in the matching `*.model.ts` so `.error({ ... })` mapping stays co-located with the schema.".into(),
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

    fn run_at(path: &str, src: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::for_test_with_framework("elysia");
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, std::path::Path::new(path), &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_error_class_in_service_file() {
        let src = "import { Elysia } from 'elysia';\nexport class NotFoundError extends Error {}";
        assert_eq!(run_at("user.service.ts", src).len(), 1);
    }

    #[test]
    fn allows_error_class_in_model_file() {
        let src = "import { Elysia } from 'elysia';\nexport class NotFoundError extends Error {}";
        assert!(run_at("user.model.ts", src).is_empty());
    }

    #[test]
    fn allows_non_error_class_in_service_file() {
        let src = "import { Elysia } from 'elysia';\nexport class UserService { greet() { return 'hi'; } }";
        assert!(run_at("user.service.ts", src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "export class NotFoundError extends Error {}";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "user.service.ts").is_empty()
        );
    }
}
