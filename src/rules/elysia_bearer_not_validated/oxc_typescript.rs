//! elysia-bearer-not-validated — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["verifyBearer"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        // If the file contains any verify call, assume it's validated.
        if ctx.source_contains(".verify(") || ctx.source_contains("verifyBearer") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("({bearer}")
                || norm.contains("({bearer,")
                || norm.contains(",bearer}")
                || norm.contains(",bearer,")
            {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
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
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
