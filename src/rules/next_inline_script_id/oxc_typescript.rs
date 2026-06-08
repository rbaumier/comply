//! next-inline-script-id oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Script"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };
        let opening = &element.opening_element;
        let tag = match &opening.name {
            JSXElementName::IdentifierReference(id) => id.name.as_str(),
            JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if tag != "Script" {
            return;
        }

        let mut has_id = false;
        let mut has_dangerously_set_inner_html = false;
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "id" => has_id = true,
                "dangerouslySetInnerHTML" => has_dangerously_set_inner_html = true,
                _ => {}
            }
        }
        if has_id {
            return;
        }

        // Has children with real content? Skip whitespace-only text.
        let has_inline_body = has_dangerously_set_inner_html
            || element.children.iter().any(|child| match child {
                JSXChild::Text(t) => !t.value.trim().is_empty(),
                JSXChild::ExpressionContainer(_)
                | JSXChild::Element(_)
                | JSXChild::Fragment(_) => true,
                _ => false,
            });
        if !has_inline_body {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inline `<Script>` requires a stable `id` prop so Next.js can dedupe \
                      it across navigations and HMR. Add `id=\"...\"`."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_inline_script_without_id() {
        let src = r#"const x = <Script>{`console.log("x");`}</Script>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_dangerously_set_inner_html_without_id() {
        let src = r#"const x = <Script dangerouslySetInnerHTML={{ __html: "x" }} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_script_with_id() {
        let src = r#"const x = <Script id="analytics">{`console.log("x");`}</Script>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_external_script_without_id() {
        let src = r#"const x = <Script src="/x.js" />;"#;
        assert!(run(src).is_empty());
    }
}
