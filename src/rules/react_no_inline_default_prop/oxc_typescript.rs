//! react-no-inline-default-prop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["memo"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is `memo` or `React.memo`.
        let is_memo = match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => id.name == "memo",
            oxc_ast::ast::Expression::StaticMemberExpression(m) => {
                if let oxc_ast::ast::Expression::Identifier(obj) = &m.object {
                    obj.name == "React" && m.property.name.as_str() == "memo"
                } else {
                    false
                }
            }
            _ => false,
        };
        if !is_memo {
            return;
        }

        // Extract the call text and look for destructuring with non-primitive defaults.
        let call_start = call.span.start as usize;
        let call_end = call.span.end as usize;
        if call_end > ctx.source.len() {
            return;
        }
        let call_text = &ctx.source[call_start..call_end];

        let brace_open = match call_text.find('{') {
            Some(i) => i,
            None => return,
        };
        let mut depth = 0i32;
        let mut brace_close: Option<usize> = None;
        for (i, ch) in call_text[brace_open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        brace_close = Some(brace_open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = match brace_close {
            Some(c) => c,
            None => return,
        };
        let params = &call_text[brace_open..=close];
        if params.contains("= []")
            || params.contains("= {}")
            || params.contains("= () =>")
            || params.contains("= new ")
        {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.callee.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Non-primitive default prop inside `memo()` creates a new reference every render. Move it outside the component.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
