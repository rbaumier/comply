//! OxcCheck backend for elysia-better-auth-null-session — flag missing null-session check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["auth.api.getSession"])
    }

    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        // Test files stub `getSession` to return null specifically to exercise
        // the null-session path; the production null check lives in the
        // middleware under test, not in the fixture.
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        if !ctx.source_contains("auth.api.getSession") {
            return Vec::new();
        }
        if !ctx.source_contains("resolve") {
            return Vec::new();
        }
        if ctx.source_contains("status(401")
            || ctx.source_contains("!session")
            || ctx.source_contains("session === null")
            || ctx.source_contains("session == null")
        {
            return Vec::new();
        }
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Better Auth `getSession` can return null — add `if (!session) return status(401)` before using it.".into(),
            severity: Severity::Error,
            span: None,
        }]
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    

    const SRC: &str = r#"
        const plugin = new Elysia().macro({
            auth: { resolve: async () => {
                const session = await auth.api.getSession({ headers });
                return { user: session.user };
            }},
        });
    "#;

    #[test]
    fn flags_missing_null_check_in_production() {
        assert_eq!(crate::rules::test_helpers::run_rule_with_ctx(&Check, SRC, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx()).len(), 1);
    }

    #[test]
    fn skips_test_file() {
        // Regression for issue #548: a test stub returning null from getSession
        // exercises the null-session path on purpose.
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(crate::rules::test_helpers::run_rule_with_ctx(&Check, SRC, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), &file).is_empty());
    }
}
