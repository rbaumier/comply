//! no-unsafe-shell-exec OXC backend — flag shell-exec APIs whose first
//! argument is not a plain string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const UNSAFE_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];
const SAFE_RECEIVERS: &[&str] = &["Regex", "RegExp", "regex", "re", "pattern", "matcher"];

pub struct Check;

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

/// Unsafe if the argument isn't a plain string literal. Template literals
/// with substitutions are unsafe; those without are treated as plain.
fn is_unsafe_arg(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => false,
        Expression::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
        _ => true,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["exec", "spawn"])
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
        let last = name.rsplit('.').next().unwrap_or(&name);
        if !UNSAFE_FNS.contains(&last) {
            return;
        }

        // Skip safe receivers like regex.exec()
        if let Some(prefix) = name.rsplit('.').nth(1) {
            let prefix_lower = prefix.to_ascii_lowercase();
            if SAFE_RECEIVERS.iter().any(|r| prefix_lower == *r || prefix_lower.ends_with(r)) {
                return;
            }
        }

        let Some(first) = call.arguments.first() else { return };
        let Some(expr) = first.as_expression() else { return };
        if !is_unsafe_arg(expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{last}()` called with a dynamic command \u{2014} use `execFile`/`spawn` with an argv array so user input isn't re-parsed by the shell."),
            severity: Severity::Error,
            span: None,
        });
    }
}
