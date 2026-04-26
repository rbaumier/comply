//! react-no-next-headers-in-client backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::file_ctx::RscContext;

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
    if module != "next/headers" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-next-headers-in-client".into(),
        message: "`next/headers` is a server-only module. Importing it from \
                  a `\"use client\"` file throws at module evaluation."
            .into(),
        severity: Severity::Error,
        span: None,
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

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, project: &ProjectCtx, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_and_file(source, &Check, project, file)
    }

    #[test]
    fn flags_cookies_import_in_client() {
        let src = r#"
"use client";
import { cookies } from "next/headers";

export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &next_project(), &client_ctx()).len(), 1);
    }

    #[test]
    fn flags_headers_and_draft_mode() {
        let src = r#"
"use client";
import { headers, draftMode } from "next/headers";

export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &next_project(), &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_next_headers_in_server_component() {
        let src = r#"
import { cookies } from "next/headers";

export default async function Page() { return <div />; }
"#;
        assert!(run(src, &next_project(), &server_ctx()).is_empty());
    }

    #[test]
    fn allows_other_next_imports_in_client() {
        let src = r#"
"use client";
import Link from "next/link";
import { useRouter } from "next/navigation";

export default function Page() { return <div />; }
"#;
        assert!(run(src, &next_project(), &client_ctx()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = r#"
"use client";
import { cookies } from "next/headers";
"#;
        let plain = ProjectCtx::empty();
        assert!(run(src, &plain, &client_ctx()).is_empty());
    }
}
