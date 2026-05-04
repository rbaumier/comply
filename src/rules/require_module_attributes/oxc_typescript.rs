//! require-module-attributes OxcCheck backend.
//!
//! Flag import/export statements with empty `with {}` attribute clauses.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ImportDeclaration,
            AstType::ExportNamedDeclaration,
            AstType::ExportDefaultDeclaration,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["with"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (with_clause, stmt_type, _span) = match node.kind() {
            AstKind::ImportDeclaration(decl) => {
                (&decl.with_clause, "import", decl.span)
            }
            AstKind::ExportNamedDeclaration(decl) => {
                (&decl.with_clause, "export", decl.span)
            }
            _ => return,
        };

        let Some(clause) = with_clause else { return };
        if !clause.with_entries.is_empty() {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, clause.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{stmt_type} statement has an empty `with {{}}` clause \u{2014} \
                 add the required attributes or remove the clause."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
