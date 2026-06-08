//! elysia-bearer-not-validated backend — flag bearer destructure without verify.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["verifyBearer"])
    }

    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        // If the file contains any verify call, assume it's validated.
        if ctx.source_contains(".verify(") || ctx.source_contains("verifyBearer") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            // Pattern: `({ bearer })` or `({bearer,...})` in handler arg.
            let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("({bearer}")
                || norm.contains("({bearer,")
                || norm.contains(",bearer}")
                || norm.contains(",bearer,")
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "elysia-bearer-not-validated".into(),
                    message:
                        "Bearer token is destructured but never validated — any token is accepted."
                            .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
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
    fn flags_unvalidated_bearer() {
        let src =
            "import { bearer } from '@elysiajs/bearer';\napp.get('/me', ({ bearer }) => bearer);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_validated_bearer() {
        let src = "import { bearer } from '@elysiajs/bearer';\napp.get('/me', async ({ bearer }) => { const p = await jwt.verify(bearer); return p; });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/me', ({ bearer }) => bearer);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
