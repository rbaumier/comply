use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (span, pattern_len) = match node.kind() {
            AstKind::RegExpLiteral(re) => {
                (re.span, re.regex.pattern.text.as_str().len())
            }
            AstKind::CallExpression(call) => {
                let is_new_regexp = match &call.callee {
                    oxc_ast::ast::Expression::NewExpression(new_expr) => {
                        matches!(&new_expr.callee, oxc_ast::ast::Expression::Identifier(id) if id.name.as_str() == "RegExp")
                    }
                    _ => false,
                };
                if !is_new_regexp {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else { return };
                let len = match first_arg {
                    oxc_ast::ast::Argument::StringLiteral(s) => s.value.len(),
                    _ => return,
                };
                (call.span, len)
            }
            _ => return,
        };

        if pattern_len < super::MIN_PATTERN_LEN {
            return;
        }

        let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
        let row = line.saturating_sub(1);
        let lines: Vec<&str> = ctx.source.lines().collect();

        if row > 0 && lines.get(row - 1).is_some_and(|l| {
            let t = l.trim();
            t.starts_with("//") || t.starts_with("/*") || t.starts_with("*")
        }) {
            return;
        }

        let has_comment_before = semantic.comments().iter().any(|c| {
            let (cline, _) = byte_offset_to_line_col(ctx.source, c.span.start as usize);
            cline == line || cline + 1 == line
        });
        if has_comment_before {
            return;
        }

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Complex regex without a comment — add a description of what it matches.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
