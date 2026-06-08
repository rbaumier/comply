//! next-no-client-import-in-server OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use std::sync::Arc;

const CLIENT_MODULES: &[&str] = &["client-only", "react-dom/client", "react-router-dom"];

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
        if ctx.file.rsc_context != RscContext::ServerComponent {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let module = import.source.value.as_str();
        if !CLIENT_MODULES.contains(&module) {
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
                "`{module}` is browser-only — importing it into a server component breaks SSR."
            ),
            severity: Severity::Error,
            span: Some((import.span.start as usize, (import.span.end - import.span.start) as usize)),
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


    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
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
