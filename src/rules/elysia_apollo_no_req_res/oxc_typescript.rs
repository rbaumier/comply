use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_span = prop.key.span();
        let key_text = &ctx.source[key_span.start as usize..key_span.end as usize];
        if key_text != "context" {
            return;
        }

        let value_text = &ctx.source[prop.value.span().start as usize..prop.value.span().end as usize];
        // Only inspect if value is an arrow function or function expression
        if !value_text.contains("=>") && !value_text.contains("function") {
            return;
        }

        // Only inspect the parameter list, not the body.
        let Some(arrow_idx) = value_text.find("=>").or_else(|| value_text.find('{')) else { return };
        let params = &value_text[..arrow_idx];
        let norm: String = params.chars().filter(|c| !c.is_whitespace()).collect();
        if !(norm.contains("{req,") || norm.contains(",req,") || norm.contains(",req}") || norm == "{req}"
            || norm.contains("{req,res}") || norm.contains(",res}") || norm.contains("{res,"))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Apollo + Elysia context exposes `{ request }`, not `{ req, res }`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
