//! ui-exit-duration-shorter-enter OXC backend — detect `<motion.*>` JSX
//! nodes whose exit duration is longer than the enter duration.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Very small helper: find `duration: <number>` inside the raw attribute text.
fn extract_duration(text: &str) -> Option<f64> {
    let key = "duration:";
    let idx = text.find(key)?;
    let rest = text[idx + key.len()..].trim_start();
    let n: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    n.parse().ok()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["motion."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // Check tag name starts with "motion."
        let tag_span = opening.name.span();
        let tag = &ctx.source[tag_span.start as usize..tag_span.end as usize];
        if !tag.starts_with("motion.") {
            return;
        }

        let mut animate_dur: Option<f64> = None;
        let mut exit_dur: Option<f64> = None;
        let mut exit_span: Option<oxc_span::Span> = None;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            let attr_text =
                &ctx.source[attr.span.start as usize..attr.span.end as usize];
            let dur = extract_duration(attr_text);
            match name.name.as_str() {
                "animate" | "initial" | "transition" => {
                    if dur.is_some() && animate_dur.is_none() {
                        animate_dur = dur;
                    }
                }
                "exit" => {
                    exit_dur = dur;
                    exit_span = Some(attr.span);
                }
                _ => {}
            }
        }

        let (Some(enter), Some(exit)) = (animate_dur, exit_dur) else {
            return;
        };
        if exit <= enter {
            return;
        }

        let report_span = exit_span.unwrap_or(opening.span);
        let (line, column) =
            byte_offset_to_line_col(ctx.source, report_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "<{tag}> exit duration {exit}s is longer than enter duration {enter}s — dismiss will feel sluggish."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
