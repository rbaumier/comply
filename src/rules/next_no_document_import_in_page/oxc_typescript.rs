//! next-no-document-import-in-page OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_document_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("_document.")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["next/document"])
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
        if is_document_file(ctx.path) {
            return;
        }
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        if import.source.value.as_str() != "next/document" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        let range_start = import.span.start as usize;
        let range_len = (import.span.end - import.span.start) as usize;
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`next/document` may only be imported from `pages/_document.tsx`.".into(),
            severity: Severity::Error,
            span: Some((range_start, range_len)),
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


    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            &next_project())
    }


    #[test]
    fn flags_document_import_in_page() {
        let src = "import { Html } from 'next/document';";
        assert_eq!(run(src, "pages/index.tsx").len(), 1);
    }


    #[test]
    fn allows_document_import_in_document_file() {
        let src = "import { Html, Main, NextScript } from 'next/document';";
        assert!(run(src, "pages/_document.tsx").is_empty());
    }


    #[test]
    fn allows_other_imports() {
        let src = "import Link from 'next/link';";
        assert!(run(src, "pages/index.tsx").is_empty());
    }
}
