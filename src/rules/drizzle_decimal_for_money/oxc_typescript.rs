//! OXC backend for drizzle-decimal-for-money — flag `numeric('price')` /
//! `decimal('amount')` calls that don't pass `{ precision, scale }`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const MONEY_KEYWORDS: &[&str] = &[
    "price", "amount", "total", "cost", "fee", "subtotal", "balance", "salary", "wage", "tax",
    "discount", "revenue", "money",
];

fn is_money_column(name: &str) -> bool {
    let lower = name.to_lowercase();
    MONEY_KEYWORDS.iter().any(|kw| lower.contains(kw))
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

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let func_name = callee.name.as_str();
        if func_name != "numeric" && func_name != "decimal" {
            return;
        }

        // First argument must be a string literal with a money keyword.
        let Some(Argument::StringLiteral(first)) = call.arguments.first() else {
            return;
        };
        let col = first.value.as_str();
        if !is_money_column(col) {
            return;
        }

        // Check for a second argument that is an object containing `precision`.
        if let Some(second) = call.arguments.get(1)
            && let Argument::ObjectExpression(obj) = second {
                let has_precision = obj.properties.iter().any(|p| {
                    if let ObjectPropertyKind::ObjectProperty(prop) = p {
                        matches!(&prop.key, PropertyKey::StaticIdentifier(id) if id.name == "precision")
                    } else {
                        false
                    }
                });
                if has_precision {
                    return;
                }
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}('{}', ...)` for a money column needs an explicit `{{ precision, scale }}` \
                 to avoid unbounded SQL `numeric`.",
                func_name, col
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
