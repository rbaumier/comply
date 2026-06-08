//! react-no-client-only-in-server-component backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

fn module_source<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let source_node = node.child_by_field_name("source")?;
    let raw = source_node.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if module != "client-only" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-client-only-in-server-component".into(),
        message: "`client-only` throws during server render. Add `\"use client\"` \
                  or drop the import."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::FileCtx;

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", crate::project::default_static_project_ctx(), file)
    }

    #[test]
    fn flags_client_only_in_server_component() {
        let src = r#"
import "client-only";

export default async function Page() { return <div />; }
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn allows_client_only_in_client_component() {
        let src = r#"
"use client";
import "client-only";

export default function Page() { return <div />; }
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_other_imports_in_server_component() {
        let src = r#"
import "server-only";
import { cache } from "react";

export default async function Page() { return <div />; }
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }
}
