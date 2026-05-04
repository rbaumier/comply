//! zod-prefer-discriminated-union OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const TAG_KEYS: &[&str] = &["type", "kind", "__type"];

/// Check if an `z.object({...})` call argument contains a tag field with `z.literal(...)` value.
fn object_has_tag_literal(args: &oxc_ast::ast::CallExpression, source: &str) -> bool {
    // First argument should be an object expression
    let Some(first_arg) = args.arguments.first() else {
        return false;
    };
    let Argument::ObjectExpression(obj) = first_arg else {
        return false;
    };
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            continue;
        };
        let key_text = &source[prop.key.span().start as usize..prop.key.span().end as usize];
        let normalized = key_text.trim_matches(|c: char| c == '"' || c == '\'');
        if !TAG_KEYS.iter().any(|k| *k == normalized) {
            continue;
        }
        // Value must be a call to z.literal(...)
        let Expression::CallExpression(value_call) = &prop.value else {
            continue;
        };
        let callee_text =
            &source[value_call.callee.span().start as usize..value_call.callee.span().end as usize];
        if callee_text == "z.literal" {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.union"])
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

        // Callee must be z.union
        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "z.union" {
            return;
        }

        // First argument must be an array
        let Some(Argument::ArrayExpression(array)) = call.arguments.first() else {
            return;
        };

        // Check if any array element is a z.object with a tag literal
        let mut has_literal_tag = false;
        for elem in &array.elements {
            let oxc_ast::ast::ArrayExpressionElement::CallExpression(elem_call) = elem else {
                continue;
            };
            let elem_callee =
                &ctx.source[elem_call.callee.span().start as usize..elem_call.callee.span().end as usize];
            if elem_callee != "z.object" {
                continue;
            }
            if object_has_tag_literal(elem_call, ctx.source) {
                has_literal_tag = true;
                break;
            }
        }

        if !has_literal_tag {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Replace `z.union([z.object({type: z.literal(...)}), ...])` with `z.discriminatedUnion('type', [...])` for faster parsing.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
