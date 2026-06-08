use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const RESPONSE_METHODS: &[&str] = &["json", "send"];
const RESPONSE_CALLS: &[&str] = &["Response.json", "NextResponse.json"];
const ERROR_FIELD_SUFFIXES: &[&str] = &[".message", ".stack"];
const ERROR_VAR_PREFIXES: &[&str] = &["err", "error", "e"];

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        _ => None,
    }
}

fn is_response_send(name: &str) -> bool {
    if RESPONSE_CALLS.contains(&name) {
        return true;
    }
    let tail = name.rsplit('.').next().unwrap_or(name);
    RESPONSE_METHODS.contains(&tail)
}

fn text_leaks_error_details(text: &str) -> bool {
    for suffix in ERROR_FIELD_SUFFIXES {
        let mut haystack = text;
        while let Some(idx) = haystack.find(suffix) {
            let prefix = &haystack[..idx];
            let ident_end = prefix
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            let ident = &prefix[ident_end..];
            if ERROR_VAR_PREFIXES
                .iter()
                .any(|p| ident.eq_ignore_ascii_case(p))
            {
                return true;
            }
            haystack = &haystack[idx + suffix.len()..];
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
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
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Some(name) = callee_name(&call.callee) else { return };
        if !is_response_send(&name) {
            return;
        }
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if !text_leaks_error_details(args_text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-error-details-in-response".into(),
            message: "Sending `err.message`/`err.stack` to the client leaks internal details — use a generic message.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_err_message_in_res_json() {
        assert_eq!(run_on("res.json({ error: err.message })").len(), 1);
    }


    #[test]
    fn flags_err_stack_in_response_json() {
        assert_eq!(run_on("Response.json({ stack: error.stack })").len(), 1);
    }


    #[test]
    fn allows_generic_error_message() {
        assert!(run_on("res.json({ error: 'Internal Server Error' })").is_empty());
    }


    #[test]
    fn allows_err_message_in_log() {
        assert!(run_on("console.error(err.message)").is_empty());
    }
}
