//! better-auth-require-rate-limit — flag `betterAuth({ ... })` / `createAuth({ ... })`
//! whose config object lacks `rateLimit`.

use crate::diagnostic::{Diagnostic, Severity};

const AUTH_FACTORIES: &[&str] = &["betterAuth", "createAuth"];

crate::ast_check! { on ["call_expression"] prefilter = ["rateLimit"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    let fn_text = func.utf8_text(source).unwrap_or("");
    if !AUTH_FACTORIES.contains(&fn_text) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let obj = args
        .children(&mut cursor)
        .find(|c| c.kind() == "object");
    let Some(obj) = obj else { return };

    let obj_text = obj.utf8_text(source).unwrap_or("");
    if obj_text.contains("rateLimit") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Better Auth config is missing `rateLimit` — add `rateLimit: { enabled: true }` to protect auth endpoints.".into(),
        Severity::Warning,
    ));
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_missing_rate_limit() {
        assert_eq!(
            run("export const auth = betterAuth({ database: db })").len(),
            1
        );
    }

    #[test]
    fn flags_missing_rate_limit_on_create_auth() {
        assert_eq!(run("createAuth({ database: db })").len(), 1);
    }

    #[test]
    fn allows_with_rate_limit() {
        assert!(run("export const auth = betterAuth({ rateLimit: { enabled: true } })").is_empty());
    }

    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = doSomething()").is_empty());
    }
}
