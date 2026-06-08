//! prisma-select-only-needed-fields OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn object_has_key(obj: &oxc_ast::ast::ObjectExpression, source: &str, name: &str) -> bool {
    for prop in &obj.properties {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            let key_name = match &p.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name == name {
                return true;
            }
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findMany"])
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

        if !is_prisma_file(ctx.source) {
            return;
        }

        // Callee must be a member expression with `.findMany`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "findMany" {
            return;
        }

        // Find object expression arguments
        let obj_args: Vec<&oxc_ast::ast::ObjectExpression> = call
            .arguments
            .iter()
            .filter_map(|arg| match arg {
                Argument::ObjectExpression(obj) => Some(obj.as_ref()),
                _ => None,
            })
            .collect();

        if obj_args.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`findMany()` without `select` fetches every column — add `select: { ... }` for the fields you need."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        for obj in obj_args {
            if !object_has_key(obj, ctx.source, "select")
                && !object_has_key(obj, ctx.source, "include")
            {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`findMany()` is missing `select`/`include` — fetches every column."
                        .into(),
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
    fn flags_find_many_without_select() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.findMany({ where: { active: true } }); }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_find_many_with_select() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.findMany({ where: { active: true }, select: { id: true } }); }";
        assert!(run(src).is_empty());
    }
}
