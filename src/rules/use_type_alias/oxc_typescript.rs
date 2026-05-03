//! use-type-alias OxcCheck backend — detect repeated complex inline type
//! annotations via oxc AST.
//!
//! Two-pass via `run_on_semantic`: iterate all nodes collecting union/intersection
//! type text, then report duplicates.

use std::collections::HashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut annotation_lines: HashMap<String, Vec<usize>> = HashMap::new();

        for node in semantic.nodes().iter() {
            let (span, is_complex) = match node.kind() {
                AstKind::TSUnionType(u) => (u.span, true),
                AstKind::TSIntersectionType(i) => (i.span, true),
                _ => continue,
            };
            if !is_complex {
                continue;
            }

            // Skip nested union/intersection — only count the outermost.
            let parent = semantic.nodes().parent_node(node.id());
            if matches!(parent.kind(), AstKind::TSUnionType(_) | AstKind::TSIntersectionType(_)) {
                continue;
            }

            let text = &ctx.source[span.start as usize..span.end as usize];
            if text.len() <= 5 {
                continue;
            }

            let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            annotation_lines
                .entry(text.to_string())
                .or_default()
                .push(line);
        }

        let mut diagnostics = Vec::new();
        for (annotation, lines) in &annotation_lines {
            if lines.len() >= 2 {
                for &line_num in lines {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: line_num,
                        column: 1,
                        rule_id: "use-type-alias".into(),
                        message: format!(
                            "Inline type `{}` appears {} times \u{2014} extract a type alias.",
                            annotation,
                            lines.len()
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics.sort_by_key(|d| d.line);
        diagnostics
    }
}
