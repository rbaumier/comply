use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_unsafe_deserializer(name: &str) -> bool {
    matches!(
        name,
        "unserialize"
            | "deserialize"
            | "nodeSerialize.unserialize"
            | "serialize.unserialize"
            | "yaml.load"
            | "YAML.load"
            | "pickle.loads"
            | "pickle.load"
    )
}

fn looks_like_user_input(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("req.body")
        || lower.contains("req.query")
        || lower.contains("req.params")
        || lower.contains("req.headers")
        || lower.contains("req.cookies")
        || lower.contains("request.body")
        || lower.contains("request.query")
        || lower.contains("ctx.request")
        || lower.contains("event.body")
        || lower.contains("userinput")
        || lower.contains("user_input")
        || lower.contains("untrusted")
}

/// Extract function name from a call expression callee.
fn callee_name(callee: &Expression) -> Option<String> {
    match callee {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            let obj_name = match &member.object {
                Expression::Identifier(id) => id.name.as_str(),
                _ => return None,
            };
            Some(format!("{}.{}", obj_name, member.property.name))
        }
        _ => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["unserialize", "deserialize", "yaml", "YAML", "pickle"])
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

        let Some(name) = callee_name(&call.callee) else {
            return;
        };
        if !is_unsafe_deserializer(&name) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let span = first_arg.span();
        let arg_text = ctx
            .source
            .get(span.start as usize..span.end as usize)
            .unwrap_or("");
        if !looks_like_user_input(arg_text) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}` on user-controlled input enables remote code execution \u{2014} use a safe parser."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
