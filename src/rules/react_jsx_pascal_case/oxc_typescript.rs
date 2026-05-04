//! react-jsx-pascal-case oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_pascal_case(name: &str) -> bool {
    for segment in name.split('.') {
        if segment.is_empty() {
            return false;
        }
        let first = segment.chars().next().unwrap();
        if !first.is_ascii_uppercase() {
            return false;
        }
        if segment.contains('_') || segment.contains('-') {
            return false;
        }
    }
    true
}

fn is_intrinsic(name: &str) -> bool {
    let first = name.chars().next().unwrap_or('a');
    first.is_ascii_lowercase()
}

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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let tag = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str().to_string(),
            JSXElementName::IdentifierReference(id) => id.name.as_str().to_string(),
            JSXElementName::MemberExpression(member) => {
                // Reconstruct Foo.Bar from member expression.
                let span = member.span;
                let start = span.start as usize;
                let end = span.end as usize;
                if end <= ctx.source.len() {
                    ctx.source[start..end].to_string()
                } else {
                    return;
                }
            }
            JSXElementName::NamespacedName(ns) => {
                format!("{}:{}", ns.namespace.name, ns.name.name)
            }
            _ => return,
        };

        if is_intrinsic(&tag) {
            return;
        }

        if !is_pascal_case(&tag) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.name.span().start as usize);

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Component `{tag}` is not PascalCase — rename to PascalCase."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
