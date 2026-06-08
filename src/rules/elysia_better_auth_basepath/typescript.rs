//! elysia-better-auth-basepath backend — flag empty/'/' basePath in betterAuth config.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "betterAuth" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    let invalid = norm.contains("basePath:''")
        || norm.contains("basePath:\"\"")
        || norm.contains("basePath:'/'")
        || norm.contains("basePath:\"/\"");
    if !invalid {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-better-auth-basepath".into(),
        message: "`betterAuth` `basePath` cannot be empty or `'/'` — set a real prefix like `'/api/auth'`.".into(),
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
    fn flags_empty_basepath() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: '' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_root_basepath() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: '/' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_double_quoted_root() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: \"/\" });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_real_basepath() {
        let src = "import { betterAuth } from 'better-auth';\nexport const auth = betterAuth({ basePath: '/api/auth' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_better_auth_files() {
        let src = "export const auth = betterAuth({ basePath: '' });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
