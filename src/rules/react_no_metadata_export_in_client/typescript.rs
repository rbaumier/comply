//! react-no-metadata-export-in-client backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::file_ctx::RscContext;

fn extracted_name<'a>(export: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(source)
                {
                    return Some(name);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut ic = child.walk();
                for decl in child.children(&mut ic) {
                    if decl.kind() == "variable_declarator"
                        && let Some(name_node) = decl.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(source)
                    {
                        return Some(name);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if ctx.file.rsc_context != RscContext::ClientComponent {
        return;
    }
    if node.kind() != "export_statement" {
        return;
    }
    let Some(name) = extracted_name(node, source) else { return };
    if name != "metadata" && name != "generateMetadata" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-metadata-export-in-client".into(),
        message: format!(
            "`{name}` is a Next.js metadata export and is ignored in \
             `\"use client\"` files. Move it to a server component."
        ),
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
    fn flags_metadata_const_in_client() {
        let src = r#"
"use client";
export const metadata = { title: "x" };
export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &next_project(), &client_ctx()).len(), 1);
    }

    #[test]
    fn flags_generate_metadata_in_client() {
        let src = r#"
"use client";
export async function generateMetadata() {
    return { title: "x" };
}
export default function Page() { return <div />; }
"#;
        assert_eq!(run(src, &next_project(), &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_metadata_in_server_component() {
        let src = r#"
export const metadata = { title: "x" };
export default function Page() { return <div />; }
"#;
        assert!(run(src, &next_project(), &server_ctx()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = r#"
"use client";
export const metadata = { title: "x" };
"#;
        let plain = ProjectCtx::empty();
        assert!(run(src, &plain, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_other_exports_in_client() {
        let src = r#"
"use client";
export const title = "Hello";
export function useFoo() { return 1; }
export default function Page() { return <div />; }
"#;
        assert!(run(src, &next_project(), &client_ctx()).is_empty());
    }
}
