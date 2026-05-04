//! jsdoc-missing-example OxcCheck backend — every JSDoc on an exported function
//! must contain an `@example` tag.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportNamedDeclaration(export) = node.kind() else {
            return;
        };

        // Only care about exported function declarations.
        let Some(decl) = &export.declaration else {
            return;
        };
        let oxc_ast::ast::Declaration::FunctionDeclaration(func) = decl else {
            return;
        };

        let export_start = export.span.start as u32;

        // Find a JSDoc comment preceding this export.
        let Some(jsdoc_text) = find_jsdoc_above(semantic, ctx.source, export_start) else {
            // No JSDoc — that's jsdoc-on-exported's job, not ours.
            return;
        };

        if jsdoc_text.contains("@example") {
            return;
        }

        let name = func
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("<anonymous>");

        let (line, column) = byte_offset_to_line_col(ctx.source, export_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "jsdoc-missing-example".into(),
            message: format!(
                "JSDoc on `{name}` is missing `@example`. Add a real call \
                 and its return value — examples are the fastest way for \
                 callers to understand the API."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Find a JSDoc comment (`/** ... */`) immediately above a given byte position.
fn find_jsdoc_above<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &'a str,
    export_start: u32,
) -> Option<&'a str> {
    // Scan comments for the closest `/** ... */` that ends before export_start.
    let mut best: Option<(u32, &str)> = None;
    for comment in semantic.comments() {
        // comment span includes the markers
        let c_start = comment.span.start;
        let c_end = comment.span.end;
        if c_end > export_start {
            continue;
        }
        let text = &source[c_start as usize..c_end as usize];
        if !text.starts_with("/**") {
            continue;
        }
        // Keep the closest one before the export.
        if best.is_none_or(|(prev_end, _)| c_end > prev_end) {
            best = Some((c_end, text));
        }
    }

    let (end, text) = best?;
    // Only match if the comment is directly adjacent (only whitespace between).
    let between = &source[end as usize..export_start as usize];
    if between.trim().is_empty() {
        Some(text)
    } else {
        None
    }
}
