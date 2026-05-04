//! react-no-unescaped-entities oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const PROBLEMATIC: &[char] = &['"', '\'', '}'];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // JSXText has no AstType variant; we use run_on_semantic instead.
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
                for ch in PROBLEMATIC {
                    if value.contains(*ch) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, text.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Unescaped `{ch}` in JSX text — use the HTML entity instead."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        // Report once per node, not once per character.
                        break;
                    }
                }
            }
        }

        diagnostics
    }
}
