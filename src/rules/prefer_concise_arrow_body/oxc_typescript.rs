use crate::diagnostic::Diagnostic;
use crate::rules::backend::{OxcCheck, oxc_diagnostic};
use oxc_ast::AstKind;
use oxc_ast::ast::Statement;
use oxc_semantic::Semantic;

use super::META;

pub struct Check;

impl OxcCheck for Check {
    fn check(&self, semantic: &Semantic, file: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let source = semantic.source_text();

        for node in semantic.nodes().iter() {
            let AstKind::ArrowFunctionExpression(arrow) = node.kind() else {
                continue;
            };

            // Already concise (`() => expr`): nothing to collapse.
            if arrow.expression {
                continue;
            }

            // A directive prologue (e.g. "use strict") would be lost on collapse.
            if !arrow.body.directives.is_empty() {
                continue;
            }

            // The block must hold exactly one statement: a `return` with a value.
            if arrow.body.statements.len() != 1 {
                continue;
            }
            let Statement::ReturnStatement(ret) = &arrow.body.statements[0] else {
                continue;
            };
            if ret.argument.is_none() {
                continue;
            }

            // Collapsing drops anything that isn't the returned expression, so skip
            // blocks containing comments rather than suggest a lossy rewrite.
            let span = arrow.body.span;
            let body_src: &str = &source[span.start as usize..span.end as usize];
            if body_src.contains("//") || body_src.contains("/*") {
                continue;
            }

            diagnostics.push(oxc_diagnostic(
                semantic,
                META.id,
                span,
                "Block-bodied arrow returns a single value; use a concise body",
                file,
            ));
        }

        diagnostics
    }
}
