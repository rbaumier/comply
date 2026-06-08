//! node-handle-callback-err OXC backend — flag callback error params that are
//! never used.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, FormalParameter};
use std::sync::Arc;

pub struct Check;

fn is_error_param(name: &str) -> bool {
    name == "err" || name == "error" || name == "e"
}

/// Check if the function body source text references the given parameter name
/// as a standalone identifier.
fn body_uses_param(body_text: &str, param_name: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = body_text[start..].find(param_name) {
        let abs = start + pos;
        let before_ok = abs == 0 || {
            let prev = body_text.as_bytes()[abs - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        let after_ok = {
            let after = abs + param_name.len();
            after >= body_text.len() || {
                let next = body_text.as_bytes()[after];
                !next.is_ascii_alphanumeric() && next != b'_'
            }
        };
        if before_ok && after_ok {
            return true;
        }
        start = abs + param_name.len();
    }
    false
}

fn first_param_name<'a>(params: &'a [FormalParameter<'a>]) -> Option<&'a str> {
    let param = params.first()?;
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (params, body_span) = match node.kind() {
            AstKind::Function(func) => {
                let body = func.body.as_ref();
                let Some(body) = body else { return };
                (&func.params, body.span)
            }
            AstKind::ArrowFunctionExpression(arrow) => (&arrow.params, arrow.body.span),
            _ => return,
        };

        let Some(param_name) = first_param_name(&params.items) else {
            return;
        };

        if !is_error_param(param_name) || param_name.starts_with('_') {
            return;
        }

        let body_text =
            &ctx.source[body_span.start as usize..body_span.end as usize];

        if !body_uses_param(body_text, param_name) {
            let span = match node.kind() {
                AstKind::Function(func) => func.span,
                AstKind::ArrowFunctionExpression(arrow) => arrow.span,
                _ => return,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Callback error parameter `{param_name}` is declared but never used — handle the error."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_unused_err_param() {
        let d = run_on("function handle(err, data) { console.log(data); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("err"));
    }


    #[test]
    fn flags_unused_error_param() {
        let d = run_on("const fn = (error, result) => { return result; };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("error"));
    }


    #[test]
    fn allows_used_err_param() {
        assert!(run_on("function handle(err, data) { if (err) throw err; }").is_empty());
    }


    #[test]
    fn allows_used_error_param_in_arrow() {
        assert!(run_on("const fn = (error) => { console.error(error); };").is_empty());
    }


    #[test]
    fn allows_non_error_param() {
        assert!(run_on("function handle(result) { return result; }").is_empty());
    }


    #[test]
    fn allows_underscore_prefix() {
        // _err is intentionally unused — should not be flagged (but our check
        // only matches "err", "error", "e" — `_err` doesn't match).
        assert!(run_on("function handle(_err, data) { return data; }").is_empty());
    }
}
