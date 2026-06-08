//! elysia-jwt-secret-hardcoded backend — flag hardcoded JWT secret literals.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["jwt"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "jwt" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");

    // Find `secret:` and look at the following token.
    let Some(off) = args_text.find("secret:") else { return };
    let after = args_text[off + "secret:".len()..].trim_start();

    // Hardcoded: starts with quote and is not env access.
    let starts_with_string = after.starts_with('\'') || after.starts_with('"') || after.starts_with('`');
    if !starts_with_string {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-jwt-secret-hardcoded".into(),
        message: "JWT secret is a hardcoded string literal — load from `process.env` instead.".into(),
        severity: Severity::Error,
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
    fn flags_hardcoded_secret() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: 'my-super-secret' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_env_secret() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'jwt', secret: process.env.JWT_SECRET! }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "jwt({ secret: 'literal' });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
