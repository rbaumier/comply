//! next-no-script-component-in-head oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn jsx_name<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn find_script_descendants<'a>(
    child: &'a JSXChild<'a>,
    out: &mut Vec<&'a oxc_ast::ast::JSXElement<'a>>,
) {
    let JSXChild::Element(elem) = child else { return };
    if jsx_name(&elem.opening_element.name) == Some("Script") {
        out.push(elem);
    }
    for c in &elem.children {
        find_script_descendants(c, out);
    }
}

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
        if jsx_name(&element.opening_element.name) != Some("Head") {
            return;
        }
        let mut scripts: Vec<&oxc_ast::ast::JSXElement> = Vec::new();
        for child in &element.children {
            find_script_descendants(child, &mut scripts);
        }
        for s in scripts {
            let (line, column) = byte_offset_to_line_col(
                ctx.source,
                s.opening_element.span.start as usize,
            );
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`<Script>` inside `<Head>` breaks Next's loading strategy. \
                          Render `<Script>` at body level."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
