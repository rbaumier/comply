//! next-no-server-import-in-client backend.
//!
//! Flags imports from server-only modules inside files classified as
//! client components. The set covers Node built-ins that aren't browser
//! safe and Next.js server-side modules.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::file_ctx::RscContext;

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

fn module_source<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let source_node = node.child_by_field_name("source")?;
    let raw = source_node.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if ctx.file.rsc_context != RscContext::ClientComponent {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if !SERVER_MODULES.contains(&module) {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-server-import-in-client".into(),
        message: format!(
            "`{module}` is server-only and will throw or break the bundle in a `\"use client\"` file."
        ),
        severity: Severity::Error,
        span: Some((range.start, range.len())),
    });
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
        crate::rules::test_helpers::run_tsx_with_project_and_file(
            source,
            &Check,
            &next_project(),
            file,
        )
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
