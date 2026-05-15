//! drizzle-no-update-without-where oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn chain_contains_method<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    target_method: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::StaticMemberExpression(_) => {
                current_id = parent_id;
            }
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(m) = &call.callee
                    && m.property.name.as_str() == target_method
                {
                    return true;
                }
                current_id = parent_id;
            }
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".update("])
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
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "update" {
            return;
        }
        if call.arguments.is_empty() {
            return;
        }
        let receiver_ok = match &member.object {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                name == "db"
                    || name == "tx"
                    || name == "transaction"
                    || name.ends_with("Db")
                    || name.ends_with("Tx")
            }
            _ => false,
        };
        if !receiver_ok {
            return;
        }
        // `.update(table)` itself must be followed by `.set(...)` to count
        // — bare `.update()` calls on non-Drizzle receivers shouldn't
        // trip the rule. Require BOTH .set and check for missing .where.
        if !chain_contains_method(node, semantic, "set") {
            return;
        }
        if chain_contains_method(node, semantic, "where") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bulk `.update(table).set(...)` without `.where(...)` overwrites \
                      every row. Add a `.where(...)` filter or document the bulk \
                      intent."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_update_set_without_where() {
        let src = r#"const r = await db.update(users).set({ active: false });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_update_set_with_where() {
        let src = r#"const r = await db.update(users).set({ active: false }).where(eq(users.id, 1));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_update_without_set() {
        // Non-Drizzle bare `.update()` — receiver could be anything.
        let src = r#"const r = db.update(x);"#;
        assert!(run(src).is_empty());
    }
}
