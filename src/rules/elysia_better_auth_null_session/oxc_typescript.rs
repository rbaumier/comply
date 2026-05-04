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
