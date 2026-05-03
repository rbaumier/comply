//! ts-prefer-for-of OxcCheck backend — flag `for (let i = 0; i < arr.length; i++)`
//! loops where `i` is only used as `arr[i]` (never assigned or used standalone).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentOperator, BinaryOperator, Expression, ForStatementInit, SimpleAssignmentTarget,
    UnaryOperator, UpdateOperator,
};
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForStatement(for_stmt) = node.kind() else {
            return;
        };

        // 1. Init: `let i = 0` or `var i = 0`
        let Some(ForStatementInit::VariableDeclaration(init)) = &for_stmt.init else {
            return;
        };
        if init.declarations.len() != 1 {
            return;
        }
        let decl = &init.declarations[0];
        let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &decl.id else {
            return;
        };
        let idx_name = binding.name.as_str();

        // Check init value is 0
        let Some(init_val) = &decl.init else {
            return;
        };
        let Expression::NumericLiteral(num) = init_val else {
            return;
        };
        if num.value != 0.0 {
            return;
        }

        // 2. Condition: `i < arr.length`
        let Some(condition) = &for_stmt.test else {
            return;
        };
        let Expression::BinaryExpression(bin) = condition else {
            return;
        };
        if bin.operator != BinaryOperator::LessThan {
            return;
        }
        let Expression::Identifier(left_ident) = &bin.left else {
            return;
        };
        if left_ident.name.as_str() != idx_name {
            return;
        }
        // Right should be `something.length`
        let Expression::StaticMemberExpression(member) = &bin.right else {
            return;
        };
        if member.property.name.as_str() != "length" {
            return;
        }
        let arr_text = &ctx.source[member.object.span().start as usize..member.object.span().end as usize];

        // 3. Increment: `i++` or `++i` or `i += 1`
        let Some(update) = &for_stmt.update else {
            return;
        };
        let valid_inc = match update {
            Expression::UpdateExpression(upd) => {
                matches!(upd.operator, UpdateOperator::Increment)
                    && match &upd.argument {
                        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                            id.name.as_str() == idx_name
                        }
                        _ => false,
                    }
            }
            Expression::AssignmentExpression(asgn) => {
                asgn.operator == AssignmentOperator::Addition
                    && match &asgn.left {
                        oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) => {
                            id.name.as_str() == idx_name
                        }
                        _ => false,
                    }
                    && matches!(&asgn.right, Expression::NumericLiteral(n) if n.value == 1.0)
            }
            _ => false,
        };
        if !valid_inc {
            return;
        }

        // 4. Check that in the body, the index var is only used as arr[i]
        let Some(body) = &for_stmt.body.span().start.checked_sub(0) else {
            return;
        };
        let body_text = &ctx.source[for_stmt.body.span().start as usize..for_stmt.body.span().end as usize];
        let pattern_bracket = format!("{arr_text}[{idx_name}]");
        let cleaned = body_text.replace(&pattern_bracket, "");
        // Check if idx_name still appears as a word boundary identifier
        let idx_bytes = idx_name.as_bytes();
        let cleaned_bytes = cleaned.as_bytes();
        let idx_len = idx_bytes.len();
        let mut still_used = false;
        for pos in 0..cleaned_bytes.len() {
            if pos + idx_len > cleaned_bytes.len() {
                break;
            }
            if &cleaned_bytes[pos..pos + idx_len] == idx_bytes {
                let before_ok = pos == 0
                    || !cleaned_bytes[pos - 1].is_ascii_alphanumeric()
                        && cleaned_bytes[pos - 1] != b'_';
                let after_ok = pos + idx_len == cleaned_bytes.len()
                    || !cleaned_bytes[pos + idx_len].is_ascii_alphanumeric()
                        && cleaned_bytes[pos + idx_len] != b'_';
                if before_ok && after_ok {
                    still_used = true;
                    break;
                }
            }
        }
        if still_used {
            return;
        }

        let span = for_stmt.span;
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `for-of` instead of an index-only `for` loop.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
