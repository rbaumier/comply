use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@deprecated"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let program = semantic.nodes().program();
        let comments = semantic.comments();
        let mut diagnostics = Vec::new();

        for stmt in &program.body {
            let Statement::ExportNamedDeclaration(export) = stmt else {
                continue;
            };
            let Some(ref decl) = export.declaration else {
                continue;
            };

            let handler_name = match decl {
                Declaration::FunctionDeclaration(f) => {
                    let Some(ref id) = f.id else { continue };
                    let name = id.name.as_str();
                    if !HTTP_METHODS.contains(&name) {
                        continue;
                    }
                    name
                }
                Declaration::VariableDeclaration(v) => {
                    let mut found = None;
                    for d in &v.declarations {
                        if let BindingPattern::BindingIdentifier(ref id) = d.id
                            && HTTP_METHODS.contains(&id.name.as_str()) {
                                found = Some(id.name.as_str());
                                break;
                            }
                    }
                    let Some(name) = found else { continue };
                    name
                }
                _ => continue,
            };

            // Check if preceding comment contains @deprecated.
            if !is_deprecated(export.span.start, comments, ctx.source) {
                continue;
            }

            // Check if body mentions "Deprecation" or "Sunset".
            let export_text =
                &ctx.source[export.span.start as usize..export.span.end as usize];
            if export_text.contains("Deprecation") || export_text.contains("Sunset") {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, export.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Deprecated `{handler_name}` handler must set `Deprecation` and `Sunset` response headers so clients can detect the deprecation at runtime."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// Check if there is a comment containing `@deprecated` just before the given byte offset.
fn is_deprecated(export_start: u32, comments: &[oxc_ast::Comment], source: &str) -> bool {
    // Find comments that end before the export starts.
    for comment in comments.iter().rev() {
        if comment.span.end > export_start {
            continue;
        }
        // Check the text between comment end and export start — should be whitespace only.
        let between = &source[comment.span.end as usize..export_start as usize];
        if !between.trim().is_empty() {
            break;
        }
        let text = &source[comment.span.start as usize..comment.span.end as usize];
        if text.contains("@deprecated") {
            return true;
        }
        // Keep looking at preceding comments (multi-line JSDoc).
    }
    false
}
