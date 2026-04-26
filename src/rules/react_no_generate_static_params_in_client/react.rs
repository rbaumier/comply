//! react-no-generate-static-params-in-client backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::file_ctx::RscContext;

fn exports_function_named(
    export: tree_sitter::Node,
    source: &[u8],
    target: &str,
) -> bool {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        if child.kind() != "function_declaration" {
            continue;
        }
        if let Some(name_node) = child.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
            && name == target
        {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if ctx.file.rsc_context != RscContext::ClientComponent {
        return;
    }
    if !exports_function_named(node, source, "generateStaticParams") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-generate-static-params-in-client".into(),
        message: "`generateStaticParams` only runs in server components. \
                  Move it out of this `\"use client\"` file or the build \
                  silently skips pre-rendering."
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
    fn flags_generate_static_params_in_client() {
        let src = r#"
"use client";
export async function generateStaticParams() {
    return [{ slug: "a" }];
}
export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &next_project(), &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_generate_static_params_in_server() {
        let src = r#"
export async function generateStaticParams() {
    return [{ slug: "a" }];
}
export default function Page() { return <div />; }
"#;
        assert!(run(src, &next_project(), &server_ctx()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = r#"
"use client";
export async function generateStaticParams() { return []; }
"#;
        let plain = ProjectCtx::empty();
        assert!(run(src, &plain, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_other_exports_in_client() {
        let src = r#"
"use client";
export function helper() { return 1; }
export default function Page() { return <div />; }
"#;
        assert!(run(src, &next_project(), &client_ctx()).is_empty());
    }
}
