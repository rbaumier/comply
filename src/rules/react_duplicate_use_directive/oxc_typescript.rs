//! react-duplicate-use-directive oxc backend.
//!
//! Fires once per file when `ctx.file.directives` captures both flags.

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
        if !(ctx.file.directives.use_client && ctx.file.directives.use_server) {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "This file has both `\"use client\"` and `\"use server\"`. \
                      Only the first directive takes effect — pick one."
                .into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}
