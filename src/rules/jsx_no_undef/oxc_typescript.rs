//! jsx-no-undef OXC backend — walk every `JSXOpeningElement` and flag
//! PascalCase tag identifiers that don't resolve to any symbol in the file.

use std::collections::HashSet;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;

pub struct Check;

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let defined: HashSet<String> = scoping.symbol_names().map(|s| s.to_string()).collect();

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::JSXOpeningElement(opening) = node.kind() else { continue };
            let (name, span_start) = match &opening.name {
                JSXElementName::IdentifierReference(ident) => {
                    (ident.name.as_str(), ident.span.start as usize)
                }
                JSXElementName::Identifier(ident) => {
                    (ident.name.as_str(), ident.span.start as usize)
                }
                _ => continue,
            };

            if !starts_with_uppercase(name) {
                continue;
            }

            if defined.contains(name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{name}` is not defined."),
                severity: Severity::Error,
                span: None,
            });
        }

        diagnostics
    }
}
