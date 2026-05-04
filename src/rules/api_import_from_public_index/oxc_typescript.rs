//! api-import-from-public-index oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        let import_path = import.source.value.as_str();

        // Only cross-feature imports (2+ parent segments).
        let parent_count = import_path.split('/').filter(|s| *s == "..").count();
        if parent_count < 2 {
            return;
        }

        // A bare feature-root import (`../../users`) has exactly one
        // non-`..` segment — the feature name — and that *is* the public
        // index. Anything deeper (`../../users/db/queries`) has 2+ and is
        // reaching into internals.
        let non_parent_segments: Vec<&str> = import_path
            .split('/')
            .filter(|s| *s != ".." && !s.is_empty())
            .collect();
        if non_parent_segments.len() <= 1 {
            return;
        }

        // Flag if the import doesn't end at an index file.
        let last_segment = *non_parent_segments.last().unwrap_or(&"");
        if last_segment == "index" {
            return;
        }
        // Skip obvious shared-leaf imports.
        if last_segment == "types" || last_segment == "utils" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from `{import_path}` crosses a feature boundary — import from the public index instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
