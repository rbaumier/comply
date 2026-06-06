//! no-confidential-logging OXC backend — flag logging calls containing
//! sensitive identifiers (password, token, apiKey, etc.).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, TemplateLiteral};
use oxc_span::GetSpan;
use std::sync::Arc;

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug"];

const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "apikey",
    "api_key",
    "authorization",
    "credential",
    "ssn",
    "creditcard",
    "credit_card",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["console", "logger"])
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

        if !is_logging_callee(&call.callee) {
            return;
        }

        if !has_sensitive_argument(&call.arguments, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Logging call contains sensitive data — redact secrets before logging.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_logging_callee(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let obj_name = match &member.object {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    let prop = member.property.name.as_str();

    if obj_name == "console" && CONSOLE_METHODS.contains(&prop) {
        return true;
    }
    if obj_name == "logger" {
        return true;
    }
    false
}

fn has_sensitive_argument(args: &[Argument], source: &str) -> bool {
    for arg in args {
        match arg {
            Argument::StringLiteral(_) => continue,
            Argument::TemplateLiteral(tpl) => {
                if template_has_sensitive_substitution(tpl, source) {
                    return true;
                }
            }
            _ => {
                let span = arg.span();
                let text = &source[span.start as usize..span.end as usize];
                let lower = text.to_ascii_lowercase();
                if SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
                    return true;
                }
            }
        }
    }
    false
}

fn template_has_sensitive_substitution(tpl: &TemplateLiteral, source: &str) -> bool {
    for expr in &tpl.expressions {
        let span = expr.span();
        let text = &source[span.start as usize..span.end as usize];
        let lower = text.to_ascii_lowercase();
        if SENSITIVE_WORDS.iter().any(|w| lower.contains(w)) {
            return true;
        }
    }
    false
}
