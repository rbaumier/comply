//! prisma-prefer-create-many-for-bulk oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client")
        || crate::oxc_helpers::source_contains(source, "PrismaClient")
        || crate::oxc_helpers::source_contains(source, "prisma.")
}

/// Walk ancestors to check if this node is inside a loop construct.
fn enclosing_is_loop<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    let prop = member.property.name.as_str();
                    if matches!(prop, "forEach" | "map") {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    let _ = source;
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_prisma_file(ctx.source) {
            return;
        }
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "create" {
            return;
        }
        if !enclosing_is_loop(node, semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`create()` called inside a loop — use `createMany({ data: [...] })` for one round-trip.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_create_in_for_loop() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(items: any[]) { for (const it of items) { await prisma.user.create({ data: it }); } }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_create_in_for_each() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(items: any[]) { items.forEach(async (it) => { await prisma.user.create({ data: it }); }); }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_single_create() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(it: any) { await prisma.user.create({ data: it }); }";
        assert!(run(src).is_empty());
    }
}
