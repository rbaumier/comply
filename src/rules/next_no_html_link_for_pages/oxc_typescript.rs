//! next-no-html-link-for-pages — OXC backend.
//! Flag `<a href="/path">` for internal routes in Next.js projects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
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

        // Must be an `<a>` tag.
        let JSXElementName::Identifier(tag) = &opening.name else { return };
        if tag.name.as_str() != "a" {
            return;
        }

        // Find href attribute value.
        let mut href_value: Option<&str> = None;
        let mut has_target = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            let attr_name = name.name.as_str();

            if attr_name == "target" {
                has_target = true;
            }

            if attr_name == "href" {
                if let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value {
                    href_value = Some(s.value.as_str());
                }
            }
        }

        if has_target {
            return;
        }

        let Some(href) = href_value else { return };
        if !href.starts_with('/') || href.starts_with("//") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `<Link>` from `next/link` for internal routes — `<a>` triggers a full reload.".into(),
            severity: Severity::Warning,
            span: Some((opening.span.start as usize, (opening.span.end - opening.span.start) as usize)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(source, &Check, project)
    }

    #[test]
    fn flags_internal_anchor() {
        let src = "export default function Nav() { return <a href='/about'>About</a>; }";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_external_anchor() {
        let src = "export default function Nav() { return <a href='https://x.com'>X</a>; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_anchor_with_target() {
        let src = "export default function Nav() { return <a href='/about' target='_blank'>About</a>; }";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export default function Nav() { return <a href='/about'>About</a>; }";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
