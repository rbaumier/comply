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
        if ctx.source.contains(".verify(") || ctx.source.contains("verifyBearer") {
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
