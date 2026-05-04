//! elysia-aot-dynamic-route OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
];

fn is_dynamic_path(text: &str, kind: &str) -> bool {
    match kind {
        "template" => text.contains("${"),
        "binary" => text.contains('+'),
        _ => false,
    }
}

fn imports_elysia(source: &str) -> bool {
    source.contains("from 'elysia'")
        || source.contains("from \"elysia\"")
        || source.contains("from 'elysia/")
        || source.contains("from \"elysia/")
        || source.contains("from '@elysiajs/")
        || source.contains("from \"@elysiajs/")
}

fn is_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.contains(".test.") || name.contains(".spec.") {
        return true;
    }
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("__tests__") | Some("__test__") | Some("tests") | Some("test")
        )
    })
}

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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !imports_elysia(ctx.source) {
            return;
        }
        if is_test_file(ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&method_name) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let (is_dynamic, arg_span) = match first_arg {
            oxc_ast::ast::Argument::TemplateLiteral(tpl) => {
                let text = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
                (is_dynamic_path(text, "template"), tpl.span)
            }
            oxc_ast::ast::Argument::BinaryExpression(bin) => {
                let text = &ctx.source[bin.span.start as usize..bin.span.end as usize];
                (is_dynamic_path(text, "binary"), bin.span)
            }
            _ => return,
        };

        if !is_dynamic {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, arg_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route path built dynamically (template literal / concatenation) — Elysia AOT can only compile static path strings. Use `:param` segments instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
