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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;



    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }


    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }


    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            &next_project())
    }


    #[test]
    fn flags_fs_import_in_client() {
        let src = "\"use client\";\nimport fs from 'fs';";
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }


    #[test]
    fn flags_next_server_import_in_client() {
        let src = "\"use client\";\nimport { NextResponse } from 'next/server';";
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }


    #[test]
    fn allows_react_import_in_client() {
        let src = "\"use client\";\nimport { useState } from 'react';";
        assert!(run(src, &client_ctx()).is_empty());
    }


    #[test]
    fn allows_fs_import_in_server() {
        let src = "import fs from 'fs';";
        let server = FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        };
        assert!(run(src, &server).is_empty());
    }
}
