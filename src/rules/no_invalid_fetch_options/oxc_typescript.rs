//! no-invalid-fetch-options OXC backend — flag `fetch()`/`new Request()`
//! calls whose options object has a `body` while method is GET or HEAD.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

enum PropValue<'a> {
    StringLiteral(&'a str),
    Other,
    Missing,
}

fn lookup_property<'a>(
    props: &'a oxc_ast::ast::ObjectExpression<'a>,
    name: &str,
    source: &'a str,
) -> PropValue<'a> {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
            PropertyKey::StringLiteral(s) => s.value.as_str().to_string(),
            _ => {
                // Try source text for computed keys
                let text =
                    &source[p.key.span().start as usize..p.key.span().end as usize];
                unquote(text).to_string()
            }
        };
        if key_name != name {
            continue;
        }
        if let Expression::StringLiteral(s) = &p.value {
            return PropValue::StringLiteral(s.value.as_str());
        }
        return PropValue::Other;
    }
    PropValue::Missing
}

fn has_spread(props: &oxc_ast::ast::ObjectExpression) -> bool {
    props
        .properties
        .iter()
        .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_)))
}

fn is_body_nullish(props: &oxc_ast::ast::ObjectExpression, _source: &str) -> bool {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name != "body" {
            continue;
        }
        return matches!(
            &p.value,
            Expression::NullLiteral(_)
        ) || matches!(&p.value, Expression::Identifier(id) if id.name.as_str() == "undefined");
    }
    false
}

fn has_body(props: &oxc_ast::ast::ObjectExpression) -> bool {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name == "body" {
            return true;
        }
    }
    false
}

fn detect_violation<'a>(
    props: &'a oxc_ast::ast::ObjectExpression<'a>,
    source: &'a str,
) -> Option<&'static str> {
    if !has_body(props) {
        return None;
    }
    if is_body_nullish(props, source) {
        return None;
    }

    let method = match lookup_property(props, "method", source) {
        PropValue::StringLiteral(s) => s.to_ascii_uppercase(),
        PropValue::Other => return None,
        PropValue::Missing => {
            if has_spread(props) {
                return None;
            }
            "GET".to_string()
        }
    };

    if method == "GET" {
        Some("GET")
    } else if method == "HEAD" {
        Some("HEAD")
    } else {
        None
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch", "Request"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::Identifier(callee) = &call.callee else {
                    return;
                };
                if callee.name.as_str() != "fetch" {
                    return;
                }
                let Some(opts_arg) = call.arguments.get(1) else {
                    return;
                };
                let obj = match opts_arg {
                    Argument::ObjectExpression(o) => o,
                    _ => return,
                };
                if let Some(method) = detect_violation(obj, ctx.source) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`body` is not allowed when method is \"{}\".",
                            method
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(callee) = &new_expr.callee else {
                    return;
                };
                if callee.name.as_str() != "Request" {
                    return;
                }
                let Some(opts_arg) = new_expr.arguments.get(1) else {
                    return;
                };
                let obj = match opts_arg {
                    Argument::ObjectExpression(o) => o,
                    _ => return,
                };
                if let Some(method) = detect_violation(obj, ctx.source) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`body` is not allowed when method is \"{}\".",
                            method
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
