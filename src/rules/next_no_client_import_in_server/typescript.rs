//! next-no-client-import-in-server backend.
//!
//! Flags imports of `client-only` and a small set of browser-only npm
//! packages inside server components. These modules touch `window` /
//! `document` at module evaluation time, which throws during SSR.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::file_ctx::RscContext;

const CLIENT_MODULES: &[&str] = &[
    "client-only",
    "react-dom/client",
    "react-router-dom",
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
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if !CLIENT_MODULES.contains(&module) {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-client-import-in-server".into(),
        message: format!(
            "`{module}` is browser-only — importing it into a server component breaks SSR."
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

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", &next_project(), file)
    }

    #[test]
    fn flags_client_only_in_server() {
        let src = "import 'client-only';";
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn allows_client_only_in_client_component() {
        let src = "\"use client\";\nimport 'client-only';";
        let client = FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        };
        assert!(run(src, &client).is_empty());
    }

    #[test]
    fn allows_server_safe_imports() {
        let src = "import { db } from '@/lib/db';";
        assert!(run(src, &server_ctx()).is_empty());
    }
}
