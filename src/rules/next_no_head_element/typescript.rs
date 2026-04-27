//! next-no-head-element backend.
//!
//! Flags raw `<head>` elements in Next.js projects. The pages router has
//! `next/head`; the app router has the metadata API. A literal `<head>`
//! element only belongs in `_document.tsx`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn tag_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("name")?.utf8_text(source).ok()
}

fn is_document_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("_document.")
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if is_document_file(ctx.path) {
        return;
    }
    let Some(tag) = tag_name(node, source) else { return };
    if tag != "head" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-head-element".into(),
        message: "Use `next/head` (pages) or the metadata API (app) instead of a raw `<head>` element.".into(),
        severity: Severity::Warning,
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

    fn run(source: &str, project: &ProjectCtx, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_file_and_path(
            source,
            &Check,
            project,
            &FileCtx::default(),
            path,
        )
    }

    #[test]
    fn flags_head_element_in_page() {
        let src = "export default function Page() { return <head><title>X</title></head>; }";
        assert_eq!(run(src, &next_project(), "pages/index.tsx").len(), 1);
    }

    #[test]
    fn allows_head_in_document() {
        let src = "export default function MyDocument() { return <head><title>X</title></head>; }";
        assert!(run(src, &next_project(), "pages/_document.tsx").is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export default function Page() { return <head /> ; }";
        assert!(run(src, &ProjectCtx::empty(), "src/page.tsx").is_empty());
    }
}
