//! OxcCheck backend for elysia-model-export-types.
//!
//! When a file exports a `t.Object(...)` const, expect a corresponding
//! `typeof X.static` type alias. Full-semantic dispatch (no per-node walk)
//! because this is a whole-file text heuristic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
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

        let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();

        let exports_typebox_const = norm.contains("exportconst") && norm.contains("=t.Object(");
        if !exports_typebox_const {
            return Vec::new();
        }

        if norm.contains(".static") {
            return Vec::new();
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, 0);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Module exports a `t.Object(...)` schema but no `typeof X.static` type — consumers cannot annotate variables with the model type.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
