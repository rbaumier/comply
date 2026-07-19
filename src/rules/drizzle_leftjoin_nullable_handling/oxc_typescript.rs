//! drizzle-leftjoin-nullable-handling oxc backend — flag `.leftJoin(...)` calls
//! without visible null handling in the surrounding statement.
//!
//! The genuine bug this rule targets is a LEFT JOIN whose joined *entity* is
//! later dereferenced as if non-null (`row.joined.field`) — the join can omit
//! the right side, so `row.joined` is `null` and the access throws. That shape
//! arises when the joined rows are reshaped in TS after a flat fetch, not when
//! they are projected through the query builder.
//!
//! A `.leftJoin(...)` that belongs to a `.select({...})` query-builder chain is
//! **structurally not that bug**: Drizzle types every left-joined column in the
//! projection as `T | null`, so the TypeScript compiler already forces the
//! consumer to handle `null` (`?? ""`, a `.nullable()` wire field, a `—`
//! placeholder). The developer cannot read the column as non-null without a
//! compile error. Firing there is a false positive (rbaumier/comply#527): the
//! rule re-demands handling the type system already mandates.
//!
//! So the rule suppresses LEFT JOINs inside a `.select(...)`/`.selectDistinct(...)`
//! chain and keeps the original single-statement token heuristic for every other
//! shape, preserving the true positive where a left-joined entity is consumed
//! without a guard.
//!
//! The suppression is **intra-statement only**: it walks the receiver chain of
//! the `.leftJoin(...)` call within the same expression. A builder split across
//! statements (`const q = db.select(...).from(t).$dynamic(); q.leftJoin(...)`)
//! presents a bare identifier as the receiver, which the walk cannot resolve to
//! the earlier `.select`, so such a chain still fires and a `comply-ignore`
//! remains the escape hatch. This is deliberate: resolving the binding would
//! require symbol-table walking beyond this syntactic rule's budget, and the
//! split-builder shape does not occur in the codebases this rule targets.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when the call chain to the left of `member` (its receiver) contains a
/// `.select(...)`/`.selectDistinct(...)` segment, i.e. the `.leftJoin(...)`
/// projects flat columns through the query builder rather than feeding a TS
/// reshape of joined entities. Walks down the left-associated `object` chain
/// only — `db.select({...}).from(t).leftJoin(...)` reaches `.select` from the
/// `.leftJoin` callee's object, `.from`, and so on.
fn chain_has_select(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    let mut receiver: &Expression = &member.object;
    loop {
        match receiver {
            Expression::CallExpression(call) => {
                if let Expression::StaticMemberExpression(inner) = &call.callee {
                    let name = inner.property.name.as_str();
                    if name == "select" || name == "selectDistinct" {
                        return true;
                    }
                    receiver = &inner.object;
                } else {
                    return false;
                }
            }
            Expression::StaticMemberExpression(inner) => {
                let name = inner.property.name.as_str();
                if name == "select" || name == "selectDistinct" {
                    return true;
                }
                receiver = &inner.object;
            }
            Expression::ParenthesizedExpression(paren) => receiver = &paren.expression,
            _ => return false,
        }
    }
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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "leftJoin" {
            return;
        }

        // The joined columns of a `.select({...})` chain are typed `T | null` by
        // Drizzle; the TS compiler already forces the consumer to handle null, so
        // demanding extra handling is a false positive (rbaumier/comply#527).
        if chain_has_select(member) {
            return;
        }

        // Get the full statement text from source using the call span as a
        // starting point. We use a heuristic: look at a wider window of source
        // around the call (from start of line to end of statement/line).
        let start = call.span.start as usize;
        let end = call.span.end as usize;

        // Walk backwards to start of line.
        let line_start = ctx.source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        // Walk forwards to end of line/statement.
        let line_end = ctx.source[end..].find('\n').map(|i| i + end).unwrap_or(ctx.source.len());
        let stmt_text = &ctx.source[line_start..line_end];

        if stmt_text.contains("?.")
            || stmt_text.contains("?? ")
            || stmt_text.contains("=== null")
            || stmt_text.contains("!== null")
            || stmt_text.contains("isNotNull(")
            || (stmt_text.contains("if (") && stmt_text.contains("!= null"))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.leftJoin(...)` produces nullable joined columns — handle `null` (filter, `??`, or `isNotNull`) before reading the joined fields.".into(),
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
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_leftjoin_without_null_handling() {
        // A bare `.leftJoin(...)` not in a `.select(...)` chain, with no null
        // handling on the statement — the original true positive.
        let src = r#"
const rows = await db.update(product).set(values).from(other)
  .leftJoin(category, eq(category.id, product.categoryId));
"#;
        assert_eq!(
            run(src).len(),
            1,
            "leftJoin outside a select chain with no null handling must still fire"
        );
    }

    #[test]
    fn allows_leftjoin_with_nullish_coalescing_on_statement() {
        // Original suppression path: `?? ` on the same statement.
        let src = r#"
const value = row.joined ?? db.update(t).from(o).leftJoin(j, eq(j.id, o.jId));
"#;
        assert!(
            run(src).is_empty(),
            "`?? ` on the leftJoin statement suppresses (original heuristic)"
        );
    }

    #[test]
    fn allows_leftjoin_inside_select_chain_intentionally_nullable() {
        // rbaumier/comply#527: the LEFT-JOINed column is projected through
        // `.select({...})`, so Drizzle types it `T | null` and the consumer
        // declares it `.nullable()` in the wire schema / renders `—`. The
        // null handling lives in another statement the single-line window
        // cannot see; the rule must not fire.
        let src = r#"
const rows = db
  .select({
    ...getColumns(productCentralCorrespondence),
    centrale: getColumns(centrale),
    doubleNet: productDoubleNetByCentrale.doubleNet,
  })
  .from(productCentralCorrespondence)
  .innerJoin(centrale, eq(productCentralCorrespondence.centraleId, centrale.id))
  .leftJoin(
    productDoubleNetByCentrale,
    and(
      eq(productDoubleNetByCentrale.productId, productCentralCorrespondence.productId),
      eq(productDoubleNetByCentrale.centraleId, productCentralCorrespondence.centraleId),
    ),
  )
  .where(eq(productCentralCorrespondence.productId, productId));
"#;
        assert!(
            run(src).is_empty(),
            "a leftJoin projected through `.select({{...}})` is intentionally nullable — must not fire"
        );
    }

    #[test]
    fn allows_leftjoin_inside_select_chain_with_cte_and_multiple_joins() {
        // The full extract-products-csv.ts shape: `.with(...).select({...})`
        // followed by an innerJoin and several leftJoins. Every leftJoin in
        // the chain is projected, so none must fire.
        let src = r#"
const rows = database
  .with(typesByProduct, speciesByProduct)
  .select({
    id: product.id,
    categoryName: category.name,
    doubleNet: productDoubleNetByCentrale.doubleNet,
  })
  .from(product)
  .innerJoin(laboratory, eq(laboratory.id, product.laboratoryId))
  .leftJoin(category, eq(category.id, product.categoryId))
  .leftJoin(typesByProduct, eq(typesByProduct.productId, product.id))
  .leftJoin(productDoubleNetByCentrale, eq(productDoubleNetByCentrale.productId, product.id));
"#;
        assert!(
            run(src).is_empty(),
            "every leftJoin in a `.select(...)` chain is intentionally nullable — none must fire"
        );
    }

    #[test]
    fn allows_leftjoin_inside_selectdistinct_chain() {
        let src = r#"
const rows = db
  .selectDistinct({ name: t.name, joinedName: j.name })
  .from(t)
  .leftJoin(j, eq(j.id, t.jId));
"#;
        assert!(
            run(src).is_empty(),
            "`.selectDistinct(...)` projects nullable columns the same way as `.select(...)`"
        );
    }

    #[test]
    fn flags_outer_leftjoin_whose_only_select_is_a_subquery_argument() {
        // Over-suppression boundary: the only `.select(...)` here lives INSIDE a
        // `.from(db.select()...)` subquery argument, not in the outer
        // `.leftJoin(...)` receiver chain. The walk follows `inner.object` only,
        // never `call.arguments`, so the outer leftJoin (a non-projection shape)
        // must still fire — the subquery's projection says nothing about how the
        // outer join's columns are consumed.
        let src = r#"
const rows = await db
  .update(product)
  .set({ name: "x" })
  .from(db.select({ id: s.id }).from(s).as("sub"))
  .leftJoin(category, eq(category.id, product.categoryId));
"#;
        assert_eq!(
            run(src).len(),
            1,
            "a `.select` confined to a `.from(subquery)` argument must not suppress the outer leftJoin"
        );
    }
}
