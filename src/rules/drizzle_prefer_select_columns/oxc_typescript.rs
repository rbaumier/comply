//! drizzle-prefer-select-columns — OXC backend.
//! Flag `db.select()` / `tx.select()` / `trx.select()` with no argument
//! followed by `.from(...)` in the chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".select("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Must be `<obj>.select()` with no arguments.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "select" {
            return;
        }
        if !call.arguments.is_empty() {
            return;
        }

        // Object must be db/tx/trx.
        let obj_name = match &member.object {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !matches!(obj_name, "db" | "tx" | "trx") {
            return;
        }

        // Walk ancestors to check if `.from(...)` appears in the chain.
        if !chain_has_from(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`db.select()` with no projection fetches every column — pass `{ col: table.col, ... }` to scope the read.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors from the `.select()` call to see if a `.from(...)` call
/// wraps it in the chain (i.e. `db.select().from(...)...`).
fn chain_has_from<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(m) = &call.callee {
                    if m.property.name.as_str() == "from" {
                        return true;
                    }
                }
            }
            // Stop at statement boundaries.
            AstKind::ExportDefaultDeclaration(_)
            | AstKind::ExportNamedDeclaration(_)
            | AstKind::VariableDeclaration(_)
            | AstKind::ReturnStatement(_) => break,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_select_no_args() {
        let src = "await db.select().from(users).limit(1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_tx_select_no_args() {
        let src = "await tx.select().from(users).where(eq(users.id, 1));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_select_with_columns() {
        let src = "await db.select({ id: users.id }).from(users);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_select_callers() {
        let src = "obj.select().from(users);";
        assert!(run(src).is_empty());
    }
}
