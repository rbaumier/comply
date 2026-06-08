//! prisma-no-findunique-without-where oxc backend — flag `findUnique` /
//! `findUniqueOrThrow` calls whose argument object literal lacks a `where` key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client") || crate::oxc_helpers::source_contains(source, "PrismaClient") || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn object_has_where_key(obj: &oxc_ast::ast::ObjectExpression) -> bool {
    for prop in &obj.properties {
        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
            let key_name = match &p.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
                _ => None,
            };
            if key_name == Some("where") {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findUnique"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !is_prisma_file(ctx.source) {
            return;
        }
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop_text = member.property.name.as_str();
        if !matches!(prop_text, "findUnique" | "findUniqueOrThrow") {
            return;
        }

        // No arguments at all.
        if call.arguments.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{prop_text}()` called without arguments — must include `{{ where: ... }}`."),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Check each object argument for a `where` key.
        for arg in &call.arguments {
            let Some(expr) = arg.as_expression() else { continue };
            let Expression::ObjectExpression(obj) = expr else { continue };
            if !object_has_where_key(obj) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("`{prop_text}()` argument is missing a `where` clause — call always resolves to null."),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_find_unique_without_where() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.findUnique({ select: { id: true } }); }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_find_unique_with_where() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.findUnique({ where: { id: 1 } }); }";
        assert!(run(src).is_empty());
    }
}
