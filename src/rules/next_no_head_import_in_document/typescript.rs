//! next-no-head-import-in-document backend.
//!
//! Flags `import Head from "next/head"` inside `_document.tsx`. The
//! Document file must use the `Head` export from `next/document`; mixing
//! the two produces duplicated head tags and stripped scripts.

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

crate::ast_check! { on ["import_statement"] prefilter = ["next/head"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if !is_document_file(ctx.path) {
        return;
    }
    let Some(module) = module_source(node, source) else { return };
    if module != "next/head" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-head-import-in-document".into(),
        message: "Use `Head` from `next/document` inside `_document.tsx`, not `next/head`.".into(),
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
    fn flags_next_head_import_in_document() {
        let src = "import Head from 'next/head';";
        assert_eq!(run(src, "pages/_document.tsx").len(), 1);
    }

    #[test]
    fn allows_next_head_import_in_page() {
        let src = "import Head from 'next/head';";
        assert!(run(src, "pages/index.tsx").is_empty());
    }

    #[test]
    fn allows_document_head_in_document() {
        let src = "import { Head, Html, Main } from 'next/document';";
        assert!(run(src, "pages/_document.tsx").is_empty());
    }
}
