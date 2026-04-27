//! next-no-img-element backend.
//!
//! Flags `<img>` JSX elements in Next.js projects. The `next/image`
//! component provides lazy loading, format negotiation, and automatic
//! sizing — using a raw `<img>` opts out of all of that.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn tag_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name("name")?.utf8_text(source).ok()
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    let Some(tag) = tag_name(node, source) else { return };
    if tag != "img" {
        return;
    }

    let pos = node.start_position();
    let range = node.byte_range();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-no-img-element".into(),
        message: "Use `<Image>` from `next/image` instead of `<img>` for automatic optimization.".into(),
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

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_project_and_file(
            source,
            &Check,
            project,
            &FileCtx::default(),
        )
    }

    #[test]
    fn flags_img_element() {
        let src = "export default function Page() { return <img src='/photo.jpg' />; }";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_next_image() {
        let src = "import Image from 'next/image';\nexport default function Page() { return <Image src='/photo.jpg' width={100} height={100} />; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export default function Page() { return <img src='/photo.jpg' />; }";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
