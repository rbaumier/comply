//! OXC backend for no-deprecated-api.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const DEPRECATED_REQUIRES: &[(&str, &str)] = &[
    (
        "domain",
        "The `domain` module is deprecated — use structured error handling instead.",
    ),
    (
        "punycode",
        "The `punycode` module is deprecated — use the userland `punycode` package.",
    ),
];

const DEPRECATED_MEMBER_CALLS: &[(&str, &str, &str)] = &[
    (
        "fs",
        "exists",
        "Use `fs.existsSync()`, `fs.stat()`, or `fs.access()` instead of `fs.exists()`.",
    ),
    ("url", "parse", "Use `new URL()` instead of `url.parse()`."),
    (
        "util",
        "isArray",
        "Use `Array.isArray()` instead of `util.isArray()`.",
    ),
    (
        "util",
        "pump",
        "Use `stream.pipeline()` or `.pipe()` instead of `util.pump()`.",
    ),
];

const DEPRECATED_MEMBER_ACCESS: &[(&str, &str, &str)] = &[
    (
        "querystring",
        "escape",
        "The `querystring` module is deprecated — use `URLSearchParams` instead.",
    ),
    (
        "process.env",
        "NODE_DEBUG",
        "Use the `util.debuglog()` API instead of reading `process.env.NODE_DEBUG` directly.",
    ),
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::NewExpression,
            AstType::CallExpression,
            AstType::StaticMemberExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::NewExpression(new_expr) => {
                if let Expression::Identifier(id) = &new_expr.callee
                    && id.name.as_str() == "Buffer" {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-deprecated-api".into(),
                            message: "Use `Buffer.from()` or `Buffer.alloc()` instead of `new Buffer()`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
            }
            AstKind::CallExpression(call) => {
                // Check require('deprecated-module')
                if let Expression::Identifier(callee_id) = &call.callee
                    && callee_id.name.as_str() == "require" {
                        if let Some(first_arg) = call.arguments.first()
                            && let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg {
                                let val = lit.value.as_str();
                                for &(module, message) in DEPRECATED_REQUIRES {
                                    if val == module {
                                        let (line, column) =
                                            byte_offset_to_line_col(ctx.source, call.span.start as usize);
                                        diagnostics.push(Diagnostic {
                                            path: Arc::clone(&ctx.path_arc),
                                            line,
                                            column,
                                            rule_id: "no-deprecated-api".into(),
                                            message: message.into(),
                                            severity: Severity::Warning,
                                            span: None,
                                        });
                                    }
                                }
                            }
                        return;
                    }

                // Check deprecated member calls like fs.exists(), url.parse()
                if let Expression::StaticMemberExpression(member) = &call.callee
                    && let Expression::Identifier(obj) = &member.object {
                        let obj_name = obj.name.as_str();
                        let prop_name = member.property.name.as_str();

                        for &(o, p, message) in DEPRECATED_MEMBER_CALLS {
                            if obj_name == o && prop_name == p {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: "no-deprecated-api".into(),
                                    message: message.into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                    }
            }
            AstKind::StaticMemberExpression(member) => {
                // Check deprecated member access like querystring.escape, process.env.NODE_DEBUG
                let prop_name = member.property.name.as_str();
                let obj_text = source_text_of_expr(&member.object, ctx.source);

                for &(o, p, message) in DEPRECATED_MEMBER_ACCESS {
                    if obj_text.as_deref() == Some(o) && prop_name == p {
                        // Skip if this member_expression is the callee of a call
                        let parent_id = semantic.nodes().parent_id(node.id());
                        if parent_id != node.id() {
                            let parent = semantic.nodes().get_node(parent_id);
                            if let AstKind::CallExpression(parent_call) = parent.kind() {
                                // If our member expression is the callee of the call, skip
                                if parent_call.span.start == member.span.start
                                    || matches!(&parent_call.callee, Expression::StaticMemberExpression(m) if m.span == member.span)
                                {
                                    return;
                                }
                            }
                        }

                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, member.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-deprecated-api".into(),
                            message: message.into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

fn source_text_of_expr<'a>(expr: &Expression<'a>, source: &str) -> Option<String> {
    let span = expr.span();
    Some(source[span.start as usize..span.end as usize].to_string())
}
