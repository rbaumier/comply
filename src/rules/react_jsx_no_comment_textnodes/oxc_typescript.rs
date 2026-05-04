//! react-jsx-no-comment-textnodes oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // JSXText has no dispatchable AstType; use run_on_semantic.
        &[AstType::JSXOpeningElement]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        use oxc_ast::AstKind;

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            if let AstKind::JSXText(text) = node.kind() {
                let value = text.value.as_str();
                let trimmed = value.trim();

                let is_line_comment = trimmed.starts_with("//") && !trimmed.starts_with("///");
                let is_block_comment = trimmed.starts_with("/*") && trimmed.ends_with("*/");

                if !is_line_comment && !is_block_comment {
                    continue;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, text.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Comment as JSX text child will be rendered as \
                              visible text. Use `{/* comment */}` instead."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
