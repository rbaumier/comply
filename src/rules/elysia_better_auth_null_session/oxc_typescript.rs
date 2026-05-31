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
        if !ctx.source.contains("auth.api.getSession") {
            return Vec::new();
        }
        if !ctx.source.contains("resolve") {
            return Vec::new();
        }
        if ctx.source.contains("status(401")
            || ctx.source.contains("!session")
            || ctx.source.contains("session === null")
            || ctx.source.contains("session == null")
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
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::test_helpers::{run_oxc_ts_with_framework, run_oxc_ts_with_framework_and_file};

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
        assert_eq!(run_oxc_ts_with_framework(SRC, &Check, "elysia").len(), 1);
    }

    #[test]
    fn skips_test_file() {
        // Regression for issue #548: a test stub returning null from getSession
        // exercises the null-session path on purpose.
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(run_oxc_ts_with_framework_and_file(SRC, &Check, "elysia", &file).is_empty());
    }
}
