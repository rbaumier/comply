//! import-dedupe OXC backend — flag duplicate specifiers within one import.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let Some(specifiers) = &import.specifiers else {
            return;
        };

        let mut seen: FxHashSet<&str> = FxHashSet::default();
        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(s) = spec else {
                continue;
            };
            // Local binding = alias if present (local), else imported name.
            let local_name = s.local.name.as_str();
            if !seen.insert(local_name) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, s.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate specifier `{local_name}` in the same import — remove the redundant entry."
                    ),
                    severity: Severity::Warning,
                    span: Some((s.span.start as usize, (s.span.end - s.span.start) as usize)),
                });
            }
        }
    }
}
