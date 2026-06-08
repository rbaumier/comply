//! next-no-img-element oxc backend — flag `<img>` JSX elements in Next.js projects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
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

        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag != "img" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `<Image>` from `next/image` instead of `<img>` for automatic optimization.".into(),
            severity: Severity::Warning,
            span: None,
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


    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            project)
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
