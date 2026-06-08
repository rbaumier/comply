//! OXC backend for tanstack-start-session-secret-min-length.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useSession"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must end with `useSession`.
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if callee_name != "useSession" {
            return;
        }

        // First argument must be an object expression.
        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::ObjectExpression(obj) = first_arg else { return };

        // Find `password` property.
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
            let key_name = p.key.name();
            let Some(key_name) = key_name else { continue };
            if key_name != "password" {
                continue;
            }

            // Only flag string/template literals.
            let text = match &p.value {
                Expression::StringLiteral(s) => s.value.as_str(),
                Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                    if let Some(q) = t.quasis.first() {
                        q.value.raw.as_str()
                    } else {
                        return;
                    }
                }
                _ => return,
            };

            let min_len = ctx.config.threshold(
                "tanstack-start-session-secret-min-length",
                "min_length",
                ctx.lang,
            );
            let inner_len = text.chars().count();
            if inner_len >= min_len {
                return;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, p.value.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`useSession` password literal is only {inner_len} chars; must be \
                     at least {min_len}. Prefer reading from an env var."
                ),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_short_literal() {
        assert_eq!(run("useSession({ password: 'too-short' });").len(), 1);
    }


    #[test]
    fn allows_long_literal() {
        assert!(
            run("useSession({ password: 'abcdefghijklmnopqrstuvwxyz0123456789' });").is_empty()
        );
    }


    #[test]
    fn allows_env_var() {
        assert!(run("useSession({ password: process.env.SECRET });").is_empty());
    }
}
