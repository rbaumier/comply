use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const STATUSES: &[&str] = &["401", "403", "404", "409", "500"];
const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

fn extract_response_block(args_text: &str) -> Option<&str> {
    let idx = args_text.find("response:")?;
    let after = &args_text[idx + "response:".len()..];
    let after = after.trim_start();
    if !after.starts_with('{') {
        return None;
    }
    let bytes = after.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&method) {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let Some(response_block) = extract_response_block(args_text) else {
            return;
        };

        for code in STATUSES {
            let status_call = format!("status({code}");
            if !args_text.contains(&status_call) {
                continue;
            }
            let key = format!("{code}:");
            if response_block.contains(&key) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Handler returns `status({code}, ...)` but `response:` schema has no `{code}:` key."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
