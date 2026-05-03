//! OxcCheck backend for xstate-no-infinite-loop.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrayExpressionElement, Expression, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        let key_name = property_key_name(&prop.key);
        if key_name.as_deref() != Some("always") {
            return;
        }

        let enclosing = enclosing_state_name(node, semantic, ctx.source);

        match &prop.value {
            Expression::ObjectExpression(obj) => {
                if is_infinite_obj(obj, ctx.source, enclosing.as_deref()) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, obj.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`always` transition has no guard and stays in the same state — this will loop forever. Add a `guard`/`cond` or target a different state.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            Expression::ArrayExpression(arr) => {
                for elem in &arr.elements {
                    if let ArrayExpressionElement::ObjectExpression(obj) = elem {
                        if is_infinite_obj(obj, ctx.source, enclosing.as_deref()) {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, obj.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: "`always` transition has no guard and stays in the same state — this will loop forever. Add a `guard`/`cond` or target a different state.".into(),
                                severity: Severity::Error,
                                span: None,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn property_key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

fn find_property<'a>(
    obj: &'a oxc_ast::ast::ObjectExpression<'a>,
    name: &str,
) -> Option<&'a oxc_ast::ast::ObjectProperty<'a>> {
    for prop_item in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop_item else {
            continue;
        };
        if property_key_name(&prop.key).as_deref() == Some(name) {
            return Some(prop);
        }
    }
    None
}

fn is_infinite_obj(
    obj: &oxc_ast::ast::ObjectExpression,
    source: &str,
    enclosing_state: Option<&str>,
) -> bool {
    // Has guard or cond? Safe.
    if find_property(obj, "guard").is_some() || find_property(obj, "cond").is_some() {
        return false;
    }

    match find_property(obj, "target") {
        None => true,
        Some(target_prop) => {
            let target_text = unquote_expr_value(&target_prop.value, source);
            match (target_text, enclosing_state) {
                (Some(t), Some(state)) => t == state,
                (None, _) => true,
                _ => false,
            }
        }
    }
}

fn unquote_expr_value<'a>(expr: &'a Expression<'a>, source: &'a str) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(s) => Some(s.value.as_str()),
        _ => {
            let text = &source[expr.span().start as usize..expr.span().end as usize];
            Some(text.trim_matches(|c: char| c == '\'' || c == '"' || c == '`'))
        }
    }
}

/// Walk up ancestors to find the enclosing state name.
/// A state name is the key of the nearest ancestor ObjectProperty whose
/// grandparent ObjectProperty has key `states`.
fn enclosing_state_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> Option<String> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        let AstKind::ObjectProperty(prop) = ancestor.kind() else {
            continue;
        };
        let key = property_key_name(&prop.key)?;

        // Check if grandparent property has key "states":
        // ObjectProperty -> ObjectExpression -> ObjectProperty
        let gp_id = semantic.nodes().parent_id(ancestor.id());
        if gp_id == ancestor.id() {
            continue;
        }
        let ggp_id = semantic.nodes().parent_id(gp_id);
        if ggp_id == gp_id {
            continue;
        }
        let ggp = semantic.nodes().get_node(ggp_id);
        if let AstKind::ObjectProperty(gp_prop) = ggp.kind() {
            if property_key_name(&gp_prop.key).as_deref() == Some("states") {
                return Some(key);
            }
        }
    }
    None
}
