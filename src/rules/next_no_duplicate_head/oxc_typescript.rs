//! next-no-duplicate-head oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn child_is_head(child: &JSXChild) -> bool {
    let JSXChild::Element(elem) = child else {
        return false;
    };
    let name = match &elem.opening_element.name {
        JSXElementName::Identifier(id) => id.name.as_str(),
        JSXElementName::IdentifierReference(id) => id.name.as_str(),
        _ => return false,
    };
    name == "Head"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Head"])
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
        let head_children: Vec<_> = element.children.iter().filter(|c| child_is_head(c)).collect();
        if head_children.len() < 2 {
            return;
        }
        // Flag every Head after the first.
        for c in head_children.iter().skip(1) {
            let JSXChild::Element(elem) = c else { continue };
            let (line, column) =
                byte_offset_to_line_col(ctx.source, elem.opening_element.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Duplicate `<Head>` element — Next.js only renders one Head \
                          per page. Merge the metadata into a single `<Head>`."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_two_head_elements() {
        let src = r#"const Page = () => <div><Head><title>A</title></Head><Head><meta /></Head></div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_head() {
        let src = r#"const Page = () => <div><Head><title>A</title></Head></div>;"#;
        assert!(run(src).is_empty());
    }
}
