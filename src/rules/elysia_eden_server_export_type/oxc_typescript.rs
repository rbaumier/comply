//! elysia-eden-server-export-type oxc backend — flag server files without
//! `export type`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }
        if !ctx.source_contains("new Elysia(") {
            return Vec::new();
        }
        if !ctx.source_contains(".listen(") {
            return Vec::new();
        }
        if ctx.source_contains("export type") {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Server entry has no `export type` — Eden Treaty cannot infer routes from this module.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
