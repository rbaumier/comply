//! xpath-injection oxc backend — flag dynamic XPath queries.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression};
use std::sync::Arc;

const XPATH_METHODS: &[&str] = &[
    "select",
    "select1",
    "evaluate",
    "selectNodes",
    "selectSingleNode",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["evaluate", "selectNodes", "selectSingleNode"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression (e.g. xpath.select, doc.evaluate)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method_name = member.property.name.as_str();
        if !XPATH_METHODS.contains(&method_name) {
            return;
        }

        // Must have at least one argument
        let Some(first_arg) = call.arguments.first() else { return };

        // Flag if first argument (XPath query) is dynamic
        let is_dynamic = match first_arg {
            Argument::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
            Argument::BinaryExpression(bin) => bin.operator == BinaryOperator::Addition,
            Argument::Identifier(_)
            | Argument::StaticMemberExpression(_)
            | Argument::ComputedMemberExpression(_) => true,
            _ => false,
        };

        if !is_dynamic {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "XPath query with dynamic input — potential XPath injection.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
