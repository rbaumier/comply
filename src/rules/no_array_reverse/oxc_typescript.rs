//! OxcCheck backend for no-array-reverse — flag `.reverse()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_array, is_array_evident_initializer};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// True when the receiver is a binding that survives the statement — an
/// identifier or a member access (`xs`, `this.items`, `a[i]`). A
/// `CallExpression` receiver (`arr.filter(...)`) is a throwaway temp, not a
/// reused lvalue, so it is not a survivor.
fn is_reused_lvalue(object: &Expression) -> bool {
    matches!(
        object,
        Expression::Identifier(_)
            | Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
    )
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["reverse"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Must be a member expression call: something.reverse()
        let Expression::StaticMemberExpression(member) = &call.callee else { return };

        if member.property.name.as_str() != "reverse" {
            return;
        }

        // Must be zero-argument call
        if !call.arguments.is_empty() {
            return;
        }

        // A bare `binding.reverse();` statement relies on the in-place mutation:
        // the result is discarded and the now-reversed receiver is read later.
        // `.toReversed()` returns a new array that would be thrown away, leaving
        // the binding un-reversed — a silent correctness bug. It is a drop-in
        // only when the result is used (assigned/returned/chained) or the
        // receiver is a throwaway temp (`arr.filter(...).reverse()`).
        let parent_is_statement =
            matches!(semantic.nodes().parent_node(node.id()).kind(), AstKind::ExpressionStatement(_));
        if parent_is_statement && is_reused_lvalue(&member.object) {
            return;
        }

        // `Array#reverse` mutates in place; a `.reverse()` on a non-array
        // receiver is an unrelated same-named method — a fluent/query-builder
        // iteration modifier (Dexie `Collection`, Knex/Kysely, lodash chains,
        // RxJS) that is non-mutating and has no `.toReversed()`. Only flag with
        // positive array evidence on the receiver: an identifier resolved to an
        // array-typed/array-initialised binding, or a call/literal receiver that
        // is a recognised fresh-array producer. Parity with the `reverse`
        // handling in `no-mutating-methods`.
        let receiver_is_array = match &member.object {
            Expression::Identifier(_) => expression_is_array(&member.object, semantic),
            other => is_array_evident_initializer(other),
        };
        if !receiver_is_array {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Array#reverse()` mutates in place — use `.toReversed()` to avoid mutation."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // === True positives: result used / temp receiver still flag ===

    #[test]
    fn flags_result_assigned() {
        // Result captured into a binding — `.toReversed()` is a drop-in. The
        // array-typed receiver carries positive array evidence.
        assert_eq!(run("function f(arr: number[]) { const rev = arr.reverse(); return rev; }").len(), 1);
    }

    #[test]
    fn flags_chained_reverse() {
        // Receiver is a throwaway `CallExpression` temp, not a reused lvalue.
        // `.filter(...)` is a recognised fresh-array producer.
        assert_eq!(run("arr.filter(x => x > 0).reverse();").len(), 1);
    }

    #[test]
    fn flags_reverse_on_array_literal_receiver() {
        // A spread array literal is a fresh array; reversing it and using the
        // result is a genuine `Array.prototype.reverse`.
        assert_eq!(run("const rev = [...x].reverse();").len(), 1);
    }

    #[test]
    fn flags_returned_reverse() {
        assert_eq!(run("function f(arr: number[]) { return arr.reverse(); }").len(), 1);
    }

    #[test]
    fn flags_reverse_as_call_argument() {
        assert_eq!(run("function f(arr: number[]) { foo(arr.reverse()); }").len(), 1);
    }

    // === False positives (issue #3956): discarded in-place reverse ===

    #[test]
    fn allows_discarded_reverse_on_identifier() {
        // Regression for #3956 — the typescript-eslint shape: a named binding
        // is reversed in place and consumed by the following loop. Replacing
        // with `.toReversed()` (a discarded new array) would silently leave
        // the binding un-reversed.
        let src = r#"
            const xs = make();
            xs.reverse();
            for (const i of xs) {}
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_discarded_reverse_on_member() {
        // Member receiver (`this.items`) — the field is mutated in place.
        assert!(run("this.items.reverse();").is_empty());
    }

    #[test]
    fn allows_discarded_reverse_on_computed_member() {
        assert!(run("rows[i].reverse();").is_empty());
    }

    // === False positives (issue #7911): non-array `.reverse()` ===

    #[test]
    fn ignores_dexie_collection_reverse_issue_7911() {
        // Regression for rbaumier/comply#7911 — a Dexie `Collection.reverse()`
        // is a non-mutating query modifier, not `Array.prototype.reverse`. The
        // receiver (`db.checkpoints.where(...)` / `db.checkpoints.toCollection()`)
        // is a call whose method is not a fresh-array producer, so it carries no
        // array evidence and must not be flagged (a Dexie `Collection` has no
        // `.toReversed()`, so the suggested fix would not even compile).
        let src = r#"
            let query;
            if (thread_id) {
                query = db.checkpoints.where({ thread_id }).reverse();
            } else {
                query = db.checkpoints.toCollection().reverse();
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_reverse_on_non_array_producing_call_issue_7911() {
        // Negative space for #7911 — a call receiver whose method is not a
        // fresh-array producer (`getList()`) may return any shape, so its
        // `.reverse()` is not evidently an `Array.prototype.reverse`.
        assert!(run("const r = obj.getList().reverse();").is_empty());
    }

    #[test]
    fn ignores_reverse_on_member_receiver_result_used_issue_7911() {
        // #7911 — a member receiver (`this.items`) has no locally-resolvable
        // element type, so `.reverse()` on it carries no array evidence even
        // when the result is used (not the discarded-in-place #3956 shape); a
        // query-builder field accessor is exactly this shape.
        assert!(run("const rev = this.items.reverse();").is_empty());
    }

    // === Unrelated / non-mutating ===

    #[test]
    fn allows_to_reversed() {
        assert!(run("const rev = arr.toReversed();").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run("const x = arr.map(x => x * 2);").is_empty());
    }
}
