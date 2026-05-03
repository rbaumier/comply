//! tanstack-query-pass-signal-to-fetch OXC backend.
//!
//! Detects a `queryFn: ({ signal }) => ...` arrow whose body calls
//! `fetch(...)` without forwarding the `signal`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, FormalParameter, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
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

        // Key must be `queryFn`
        let PropertyKey::StaticIdentifier(key) = &prop.key else {
            return;
        };
        if key.name.as_str() != "queryFn" {
            return;
        }

        // Value must be an arrow function
        let Expression::ArrowFunctionExpression(arrow) = &prop.value else {
            return;
        };

        let nodes = semantic.nodes();

        if destructures_signal(&arrow.params.items) {
            // Check if any fetch call inside doesn't pass signal
            if !has_bad_fetch_in_span(nodes, arrow.span, None) {
                return;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`queryFn` destructures `{ signal }` but does not pass it to `fetch`. \
                         Forward it: `fetch(url, { signal })` so cancellation aborts the request."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        } else if let Some(param_name) = single_identifier_param(&arrow.params.items) {
            if !has_bad_fetch_in_span(nodes, arrow.span, Some(param_name)) {
                return;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`queryFn` receives the query context but does not pass its `signal` to `fetch`. \
                         Forward it: `fetch(url, { signal: ctx.signal })` so cancellation aborts the request."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// True when the arrow's first parameter is an object pattern binding `signal`.
fn destructures_signal(params: &[FormalParameter]) -> bool {
    for param in params {
        let BindingPattern::ObjectPattern(obj) = &param.pattern else {
            continue;
        };
        for prop in &obj.properties {
            let name = match &prop.key {
                PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
                _ => continue,
            };
            if name == "signal" {
                return true;
            }
        }
    }
    false
}

/// If the arrow has exactly one parameter that is a plain identifier, return its name.
fn single_identifier_param<'a>(params: &'a [FormalParameter<'a>]) -> Option<&'a str> {
    if params.len() != 1 {
        return None;
    }
    let BindingPattern::BindingIdentifier(ident) = &params[0].pattern else {
        return None;
    };
    Some(ident.name.as_str())
}

/// Check if there's a `fetch(...)` call inside the span that doesn't pass signal.
/// If `ctx_name` is None, we look for a shorthand `signal` property.
/// If `ctx_name` is Some, we look for `signal: <ctx>.signal`.
fn has_bad_fetch_in_span(
    nodes: &oxc_semantic::AstNodes,
    span: oxc_span::Span,
    ctx_name: Option<&str>,
) -> bool {
    for node in nodes.iter() {
        let node_span = node.kind().span();
        if node_span.start < span.start || node_span.end > span.end {
            continue;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = &call.callee else {
            continue;
        };
        if callee.name.as_str() != "fetch" {
            continue;
        }

        // Check second argument for signal
        let opts = call.arguments.get(1);
        match opts {
            None => return true,
            Some(arg) => {
                let expr = match arg {
                    Argument::ObjectExpression(obj) => obj.as_ref(),
                    other => {
                        let Some(e) = other.as_expression() else {
                            continue;
                        };
                        let Expression::ObjectExpression(obj) = e else {
                            // Spread/variable — can't be sure; don't flag.
                            continue;
                        };
                        obj.as_ref()
                    }
                };

                let mut has_signal = false;
                for prop in &expr.properties {
                    let ObjectPropertyKind::ObjectProperty(p) = prop else {
                        continue;
                    };
                    let PropertyKey::StaticIdentifier(k) = &p.key else {
                        continue;
                    };
                    if k.name.as_str() != "signal" {
                        continue;
                    }
                    if let Some(ctx) = ctx_name {
                        // Check value is `<ctx>.signal`
                        let Expression::StaticMemberExpression(member) = &p.value else {
                            continue;
                        };
                        let Expression::Identifier(obj_ident) = &member.object else {
                            continue;
                        };
                        if obj_ident.name.as_str() == ctx
                            && member.property.name.as_str() == "signal"
                        {
                            has_signal = true;
                            break;
                        }
                    } else {
                        // Shorthand or pair with `signal` key
                        has_signal = true;
                        break;
                    }
                }
                if !has_signal {
                    return true;
                }
            }
        }
    }
    false
}
