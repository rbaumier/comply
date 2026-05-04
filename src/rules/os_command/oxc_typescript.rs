//! os-command OXC backend — detect potential OS command injection.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const DANGEROUS_FUNCTIONS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];
const SAFE_RECEIVERS: &[&str] = &["Regex", "RegExp", "regex", "re", "pattern", "matcher"];

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let func_name = match &call.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if !DANGEROUS_FUNCTIONS.contains(&name) {
                    return;
                }
                name.to_string()
            }
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if !DANGEROUS_FUNCTIONS.contains(&prop) {
                    return;
                }
                // Check safe receivers
                let obj_text =
                    &ctx.source[member.object.span().start as usize..member.object.span().end as usize];
                let obj_lower = obj_text.to_ascii_lowercase();
                if SAFE_RECEIVERS
                    .iter()
                    .any(|r| obj_lower == *r || obj_lower.ends_with(r))
                {
                    return;
                }
                prop.to_string()
            }
            _ => return,
        };

        // Need at least one argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        use oxc_ast::ast::Argument;
        use oxc_span::GetSpan;
        let is_dynamic = match first_arg {
            Argument::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
            Argument::BinaryExpression(bin) => {
                matches!(bin.operator, oxc_ast::ast::BinaryOperator::Addition)
            }
            Argument::Identifier(_) => true,
            Argument::StaticMemberExpression(_) | Argument::ComputedMemberExpression(_) => true,
            _ => false,
        };

        if !is_dynamic {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, first_arg.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{func_name}()` with dynamic command \u{2014} potential command injection."),
            severity: Severity::Error,
            span: None,
        });
    }
}
