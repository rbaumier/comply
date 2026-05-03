use std::sync::Arc;

use oxc_ast::ast::Expression;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::CallExpression,
            AstType::IdentifierReference,
            AstType::StaticMemberExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !crate::rules::module_system::is_es_module_context(ctx.path, ctx.project) {
            return;
        }

        match node.kind() {
            // `require("…")` calls
            AstKind::CallExpression(call) => {
                let Expression::Identifier(ident) = &call.callee else {
                    return;
                };
                if ident.name.as_str() != "require" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `import` instead of `require()` — prefer ESM over CommonJS."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // `__dirname` / `__filename` identifiers
            AstKind::IdentifierReference(ident) => {
                let msg = match ident.name.as_str() {
                    "__dirname" => {
                        "Use `import.meta.dirname` instead of `__dirname`."
                    }
                    "__filename" => {
                        "Use `import.meta.filename` instead of `__filename`."
                    }
                    _ => return,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: msg.into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // `module.exports` or `exports.foo`
            AstKind::StaticMemberExpression(member) => {
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                let obj_name = obj.name.as_str();
                let prop_name = member.property.name.as_str();

                if obj_name == "module" && prop_name == "exports" {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Use `export` instead of `module.exports` — prefer ESM over CommonJS."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                } else if obj_name == "exports" {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Use `export` instead of `exports.x = …` — prefer ESM over CommonJS."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
