//! OxcCheck backend — flag `*ngFor` without `trackBy` in Angular templates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "@Component")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_angular_file(ctx.source) {
            return;
        }
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        // Check key is "template"
        let key_name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => return,
        };
        if key_name != "template" {
            return;
        }

        // Get the template string value from source
        let value_text = match &prop.value {
            oxc_ast::ast::Expression::StringLiteral(s) => s.value.as_str().to_string(),
            oxc_ast::ast::Expression::TemplateLiteral(t) => {
                // For template literals, extract text from source span
                let start = t.span.start as usize;
                let end = t.span.end as usize;
                if end <= ctx.source.len() {
                    ctx.source[start..end].to_string()
                } else {
                    return;
                }
            }
            _ => return,
        };

        for (idx, _) in value_text.match_indices("*ngFor") {
            let tail = &value_text[idx..];
            let end = tail.find(['"', '\'']).map(|p| p + 1).unwrap_or(tail.len());
            let attr_section_end = tail[end..]
                .find(['"', '\''])
                .map(|p| end + p)
                .unwrap_or(tail.len());
            let attr_section = &tail[..attr_section_end];
            if !attr_section.contains("trackBy") {
                let span_start = match &prop.value {
                    oxc_ast::ast::Expression::StringLiteral(s) => s.span.start as usize,
                    oxc_ast::ast::Expression::TemplateLiteral(t) => t.span.start as usize,
                    _ => prop.span.start as usize,
                };
                let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`*ngFor` without `trackBy` causes Angular to recreate every DOM node when the array reference changes.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
    }
}
