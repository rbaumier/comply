//! OxcCheck backend for drizzle-returning-on-insert-update — flag
//! `db.insert(..)` / `db.update(..)` chains without `.returning()`.

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

        // Match `.insert(...)` or `.update(...)` member calls.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        let is_insert = method == "insert";
        let is_update = method == "update";
        if !is_insert && !is_update {
            return;
        }

        // Walk up the chain of `.method(...)` calls to find all chained methods.
        let (outer_span, methods) = collect_chain_methods(node, semantic, ctx.source);

        // Must contain a mutation step.
        let has_mutation = if is_insert {
            methods.contains(&"values")
        } else {
            methods.contains(&"set")
        };
        if !has_mutation {
            return;
        }

        if methods.contains(&"returning") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, outer_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Drizzle insert/update without `.returning()` — chain `.returning()` \
                      to get the inserted/updated row in a single round-trip."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk up from a `.insert(..)`/`.update(..)` call through chained
/// `.method(..)` callers and collect method names.
fn collect_chain_methods<'a>(
    start: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    _source: &str,
) -> (oxc_span::Span, Vec<&'a str>) {
    let mut methods = Vec::new();
    let mut current_id = start.id();
    let mut outer_span = match start.kind() {
        AstKind::CallExpression(c) => c.span,
        _ => oxc_span::Span::new(0, 0),
    };

    loop {
        // Pattern: current is the object of a StaticMemberExpression,
        // which is the callee of a CallExpression.
        let parent = semantic.nodes().parent_node(current_id);
        let AstKind::StaticMemberExpression(member) = parent.kind() else {
            break;
        };
        let grandparent = semantic.nodes().parent_node(parent.id());
        let AstKind::CallExpression(outer_call) = grandparent.kind() else {
            break;
        };
        methods.push(member.property.name.as_str());
        outer_span = outer_call.span;
        current_id = grandparent.id();
    }

    (outer_span, methods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_insert_without_returning() {
        assert_eq!(
            run_on("await db.insert(users).values({ name: 'Alice' })").len(),
            1
        );
    }

    #[test]
    fn flags_update_without_returning() {
        assert_eq!(
            run_on("await db.update(users).set({ active: false }).where(eq(users.id, id))").len(),
            1
        );
    }

    #[test]
    fn allows_insert_with_returning() {
        assert!(
            run_on("const [u] = await db.insert(users).values({ name: 'Alice' }).returning()")
                .is_empty()
        );
    }

    #[test]
    fn allows_update_with_returning() {
        assert!(
            run_on(
                "await db.update(users).set({ active: false }).where(eq(users.id, id)).returning()"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_insert_without_values() {
        assert!(run_on("db.insert(users);").is_empty());
    }

    #[test]
    fn ignores_unrelated_insert() {
        assert!(run_on("arr.insert(0, x)").is_empty());
    }
}
