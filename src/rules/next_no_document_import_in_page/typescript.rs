//! next-no-document-import-in-page backend.
//!
//! Flags `import … from "next/document"` outside of `pages/_document.*`.
//! `next/document` exposes `Html`, `Head`, `Main`, `NextScript` which only
//! work when rendered by Next's custom Document machinery.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn module_source<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let source_node = node.child_by_field_name("source")?;
    let raw = source_node.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

fn is_document_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("_document.")
}

crate::ast_check! { on ["import_statement"] prefilter = ["next/document"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if is_document_file(ctx.path) {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if module != "next/document" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-document-import-in-page".into(),
        message: "`next/document` may only be imported from `pages/_document.tsx`.".into(),
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

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_file_and_path(
            source,
            &Check,
            &next_project(),
            &FileCtx::default(),
            path,
        )
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
