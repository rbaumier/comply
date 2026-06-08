//! next-no-async-client-component backend.
//!
//! Same intent as `react-no-async-client-component` but scoped to the
//! Next.js framework only and triggered by the `next-` rule id. Flags
//! exported async function components in client-component files (those
//! marked with `"use client"`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::file_ctx::RscContext;

fn first_token_is_async(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else { return false };
    text.trim_start().starts_with("async ")
}

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn is_inside_export(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "export_statement" {
            return true;
        }
        if parent.kind() == "program" {
            return false;
        }
        current = parent.parent();
    }
    false
}

crate::ast_check! { on ["function_declaration"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if ctx.file.rsc_context != RscContext::ClientComponent {
        return;
    }
    if !is_inside_export(node) {
        return;
    }
    if !first_token_is_async(node, source) {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };
    if !starts_with_uppercase(name) {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-async-client-component".into(),
        message: format!(
            "`{name}` is an async client component. Drop `async` or remove `\"use client\"`."
        ),
        severity: Severity::Error,
        span: Some((range.start, range.len())),
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", &next_project(), file)
    }

    #[test]
    fn flags_async_default_export() {
        let src = "\"use client\";\nexport default async function Page() { return <div />; }";
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_sync_component() {
        let src = "\"use client\";\nexport default function Page() { return <div />; }";
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_async_in_server_component() {
        let src = "export default async function Page() { return <div />; }";
        let server = FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        };
        assert!(run(src, &server).is_empty());
    }
}
