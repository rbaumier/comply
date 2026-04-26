//! react-no-server-only-in-client backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

fn module_source<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let source_node = node.child_by_field_name("source")?;
    let raw = source_node.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ClientComponent {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if module != "server-only" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-server-only-in-client".into(),
        message: "`server-only` throws at evaluation time in client bundles. \
                  Remove `\"use client\"` or move this import to a server module."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::FileCtx;

    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_file_ctx(source, &Check, file)
    }

    #[test]
    fn flags_server_only_in_client_component() {
        let src = r#"
"use client";
import "server-only";

export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_server_only_in_server_component() {
        let src = r#"
import "server-only";

export default async function Page() { return <div />; }
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_other_imports_in_client() {
        let src = r#"
"use client";
import { useState } from "react";
import "client-only";

export default function Page() { return <div />; }
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn flags_single_quoted_import() {
        let src = r#"
"use client";
import 'server-only';

export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }
}
