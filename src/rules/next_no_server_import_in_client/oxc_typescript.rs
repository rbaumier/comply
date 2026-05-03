//! next-no-server-import-in-client OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use std::sync::Arc;

const SERVER_MODULES: &[&str] = &[
    "fs",
    "fs/promises",
    "node:fs",
    "node:fs/promises",
    "net",
    "node:net",
    "dns",
    "node:dns",
    "tls",
    "node:tls",
    "child_process",
    "node:child_process",
    "next/server",
    "server-only",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.project.framework != Framework::NextJs {
            return;
        }
        if ctx.file.rsc_context != RscContext::ClientComponent {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let module = import.source.value.as_str();
        if !SERVER_MODULES.contains(&module) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{module}` is server-only and will throw or break the bundle in a `\"use client\"` file."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
