//! OXC backend for drizzle-no-parsefloat-on-numeric.
//!
//! Drizzle returns `numeric`/`decimal` columns as `string` to preserve
//! arbitrary precision. `parseFloat(row.amount)`, `Number(row.amount)`, or unary
//! `+row.amount` followed by arithmetic reintroduces IEEE-754 rounding — the
//! exact error those column types exist to prevent.
//!
//! Resolution is **same-file only** — comply has no project-wide Drizzle schema
//! index — and deliberately conservative to hold a zero false-positive rate. The
//! rule fires only when all of the following hold in the analysed file:
//!   1. a `drizzle-orm` import is present;
//!   2. the accessed property is declared as a `numeric(...)`/`decimal(...)`
//!      column *inside a Drizzle table constructor* (`pgTable`/`mysqlTable`/...),
//!      so the name is a real DB column, not a like-named helper or DTO field;
//!   3. the reparsed member access reads off a variable that is bound, in the
//!      same file, to a Drizzle row query — this ties `order.amount` to an
//!      actual table read rather than an unrelated object that happens to share
//!      a column name; and
//!   4. the reparsed value feeds `+`/`-`/`*`/`/` arithmetic (display-only
//!      formatting is intentionally allowed).
//!
//! When the receiver cannot be tied to a Drizzle row read, the rule stays
//! silent — silence is correct under the zero-FP mandate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, BindingPattern, Expression, PropertyKey, UnaryOperator};
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// Drizzle table-constructor functions whose second argument is the column map.
const TABLE_FNS: &[&str] = &[
    "pgTable",
    "mysqlTable",
    "sqliteTable",
    "pgMaterializedView",
    "pgView",
];

pub struct Check;

/// Returns the property name of a `numeric(...)`/`decimal(...)` column field,
/// i.e. the object-property key whose value is such a call. `amount` in
/// `{ amount: numeric('amount', ...) }`.
fn numeric_column_property<'a>(prop: &'a oxc_ast::ast::ObjectProperty<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = &prop.value else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let func = callee.name.as_str();
    if func != "numeric" && func != "decimal" {
        return None;
    }
    match &prop.key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Collects the `numeric`/`decimal` column names declared inside the column-map
/// object (the second argument) of a Drizzle table constructor call.
fn collect_table_columns<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
    out: &mut FxHashSet<&'a str>,
) {
    let Expression::Identifier(callee) = &call.callee else {
        return;
    };
    if !TABLE_FNS.contains(&callee.name.as_str()) {
        return;
    }
    let Some(arg) = call.arguments.get(1).and_then(|a| a.as_expression()) else {
        return;
    };
    let Expression::ObjectExpression(obj) = arg else {
        return;
    };
    for member in &obj.properties {
        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) = member
            && let Some(name) = numeric_column_property(prop)
        {
            out.insert(name);
        }
    }
}

/// True when `op` is `+`/`-`/`*`/`/` — arithmetic that rounds a reparsed value.
fn is_arithmetic(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Addition
            | BinaryOperator::Subtraction
            | BinaryOperator::Multiplication
            | BinaryOperator::Division
    )
}

/// True when `node`'s result lands directly in an arithmetic binary expression
/// (either operand). Parenthesised wrappers are transparent.
fn feeds_arithmetic(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::BinaryExpression(bin) => is_arithmetic(bin.operator),
        AstKind::ParenthesizedExpression(_) => feeds_arithmetic(parent, semantic),
        _ => false,
    }
}

/// If `expr` is a single-hop member access `row.col`, returns `(row, col)`.
/// A Drizzle column is always one hop off the row, so deeper chains
/// (`order.meta.amount`, where `amount` lives on a nested non-column object) are
/// rejected — they are not column reads.
fn member_receiver_and_property<'a>(expr: &'a Expression<'a>) -> Option<(&'a str, &'a str)> {
    let Expression::StaticMemberExpression(member) = expr else {
        return None;
    };
    let Expression::Identifier(receiver) = &member.object else {
        return None;
    };
    Some((receiver.name.as_str(), member.property.name.as_str()))
}

/// If `expr` is `parseFloat(arg)` or `Number(arg)`, returns the single argument.
/// Drizzle decimals are strings, so a single-argument call is the reparse shape.
fn float_reparse_argument<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
) -> Option<&'a Expression<'a>> {
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let func = callee.name.as_str();
    if func != "parseFloat" && func != "Number" {
        return None;
    }
    if call.arguments.len() != 1 {
        return None;
    }
    call.arguments[0].as_expression()
}

/// Walks a member/call chain, peeling `await`/parens, and records the leftmost
/// identifier (the chain root) plus every member-access segment name in
/// left-to-right order. `db.select().from(t)` yields root `db` and segments
/// `["select", "from"]`; `db.query.orders.findFirst()` yields root `db` and
/// `["query", "orders", "findFirst"]`.
fn chain_root_and_segments<'a>(
    expr: &'a Expression<'a>,
    segments: &mut Vec<&'a str>,
) -> Option<&'a str> {
    match expr {
        Expression::AwaitExpression(a) => chain_root_and_segments(&a.argument, segments),
        Expression::ParenthesizedExpression(p) => {
            chain_root_and_segments(&p.expression, segments)
        }
        Expression::CallExpression(call) => chain_root_and_segments(&call.callee, segments),
        Expression::StaticMemberExpression(member) => {
            let root = chain_root_and_segments(&member.object, segments)?;
            segments.push(member.property.name.as_str());
            Some(root)
        }
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True when `expr` is a Drizzle row read off one of the file's db handles:
/// the query-builder shape `db.select()...from()` or the relational-API shape
/// `db.query.<table>.findFirst/findMany`. Generic `.from()`/`.query` on a
/// non-db object (`dayjs().from()`, `router.query`) is rejected, since its root
/// is not a db handle and it lacks the required Drizzle segment co-occurrence.
fn is_drizzle_query_expr(expr: &Expression, db_handles: &FxHashSet<&str>) -> bool {
    let mut segments = Vec::new();
    let Some(root) = chain_root_and_segments(expr, &mut segments) else {
        return false;
    };
    if !db_handles.contains(root) {
        return false;
    }
    let has = |name: &str| segments.contains(&name);
    // Query builder: `.from` only ever follows `.select`/`.selectDistinct`.
    let query_builder = has("from") && (has("select") || has("selectDistinct"));
    // Relational API: `db.query.<table>.findFirst()/findMany()`.
    let relational = has("query") && (has("findFirst") || has("findMany"));
    query_builder || relational
}

/// Identifiers that denote a Drizzle database handle: an identifier bound to a
/// `drizzle(...)` call, plus the conventional `db`/`database`/`tx`/`trx`
/// transaction names. Only consulted in files that already import `drizzle-orm`.
fn collect_db_handles<'a>(semantic: &oxc_semantic::Semantic<'a>) -> FxHashSet<&'a str> {
    let mut handles: FxHashSet<&str> = ["db", "database", "tx", "trx"].into_iter().collect();
    for node in semantic.nodes().iter() {
        if let AstKind::VariableDeclarator(decl) = node.kind()
            && let Some(Expression::CallExpression(call)) = &decl.init
            && let Expression::Identifier(callee) = &call.callee
            && callee.name.as_str() == "drizzle"
        {
            binding_names(&decl.id, &mut handles);
        }
    }
    handles
}

/// Binding-pattern identifier names: `const x`, `const [x]`, `const { x }` all
/// contribute names that alias the initializer's row data.
fn binding_names<'a>(pattern: &'a BindingPattern<'a>, out: &mut FxHashSet<&'a str>) {
    match pattern {
        BindingPattern::BindingIdentifier(id) => {
            out.insert(id.name.as_str());
        }
        BindingPattern::ArrayPattern(arr) => {
            for elem in arr.elements.iter().flatten() {
                binding_names(elem, out);
            }
        }
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                binding_names(&prop.value, out);
            }
        }
        BindingPattern::AssignmentPattern(assign) => {
            binding_names(&assign.left, out);
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // A drizzle-orm import is a hard prerequisite for every fire, so files
        // without it are skipped entirely. The remaining gates (a table column,
        // a db-handle-rooted row query) are verified in `run_on_semantic`.
        Some(&["drizzle-orm"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut has_drizzle_import = false;
        let mut numeric_columns: FxHashSet<&str> = FxHashSet::default();
        let db_handles = collect_db_handles(semantic);
        // Identifiers bound to a Drizzle row query in this file.
        let mut row_vars: FxHashSet<&str> = FxHashSet::default();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let src = import.source.value.as_str();
                    if src == "drizzle-orm" || src.starts_with("drizzle-orm/") {
                        has_drizzle_import = true;
                    }
                }
                AstKind::CallExpression(call) => collect_table_columns(call, &mut numeric_columns),
                AstKind::VariableDeclarator(decl) => {
                    if let Some(init) = &decl.init
                        && is_drizzle_query_expr(init, &db_handles)
                    {
                        binding_names(&decl.id, &mut row_vars);
                    }
                }
                _ => {}
            }
        }

        if !has_drizzle_import || numeric_columns.is_empty() || row_vars.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let (member_expr, report_at) = match node.kind() {
                AstKind::CallExpression(call) => match float_reparse_argument(call) {
                    Some(arg) => (arg, call.span.start),
                    None => continue,
                },
                AstKind::UnaryExpression(unary)
                    if unary.operator == UnaryOperator::UnaryPlus =>
                {
                    (&unary.argument, unary.span.start)
                }
                _ => continue,
            };

            let Some((receiver, prop)) = member_receiver_and_property(member_expr) else {
                continue;
            };
            if !row_vars.contains(receiver) || !numeric_columns.contains(prop) {
                continue;
            }
            if !feeds_arithmetic(node, semantic) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, report_at as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Reparsing the `numeric`/`decimal` column `{prop}` to a float and doing \
                     arithmetic reintroduces rounding errors — keep the string and use a decimal \
                     library (`new Decimal({receiver}.{prop})`) or compute in SQL."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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

    const PRELUDE: &str = r#"
import { pgTable, numeric, decimal } from 'drizzle-orm/pg-core';
import { db } from './db';
export const orders = pgTable('orders', {
  amount: numeric('amount', { precision: 10, scale: 2 }),
});
const order = await db.query.orders.findFirst();
"#;

    #[test]
    fn flags_parsefloat_feeding_multiplication() {
        let src = format!("{PRELUDE}\nconst total = parseFloat(order.amount) * qty;");
        assert_eq!(run(&src).len(), 1, "parseFloat on a numeric column feeding `*` should fire");
    }

    #[test]
    fn flags_number_feeding_subtraction() {
        let src = format!("{PRELUDE}\nconst net = Number(order.amount) - fee;");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_unary_plus_feeding_addition() {
        let src = format!("{PRELUDE}\nconst sum = +order.amount + shipping;");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_decimal_column_from_select() {
        let src = r#"
import { pgTable, decimal } from 'drizzle-orm/pg-core';
import { db } from './db';
export const t = pgTable('t', { price: decimal('price') });
const [row] = await db.select().from(t);
const x = parseFloat(row.price) * 2;
"#;
        assert_eq!(run(src).len(), 1, "decimal column off a select row should fire");
    }

    #[test]
    fn flags_custom_drizzle_handle() {
        // A handle bound from `drizzle(...)` is recognised beyond the `db` name.
        let src = r#"
import { pgTable, numeric } from 'drizzle-orm/pg-core';
import { drizzle } from 'drizzle-orm/node-postgres';
export const orders = pgTable('orders', { amount: numeric('amount') });
const orm = drizzle(pool);
const order = await orm.query.orders.findFirst();
const total = parseFloat(order.amount) * 2;
"#;
        assert_eq!(run(src).len(), 1, "a drizzle()-bound handle should be recognised");
    }

    #[test]
    fn allows_decimal_library_good_form() {
        let src = format!("{PRELUDE}\nconst total = new Decimal(order.amount).mul(qty);");
        assert!(run(&src).is_empty(), "decimal-library arithmetic is the good form");
    }

    #[test]
    fn ignores_non_numeric_column() {
        // `qty` is not a numeric/decimal column, so reparsing `order.qty` is fine.
        let src = format!("{PRELUDE}\nconst total = parseFloat(order.qty) * 2;");
        assert!(run(&src).is_empty(), "non-numeric column must not fire");
    }

    #[test]
    fn ignores_free_text_input_with_no_schema() {
        // No drizzle schema at all — the operand is free-text input.
        let src = "const total = parseFloat(input.qty) * 2;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_colliding_property_on_unrelated_receiver() {
        // `form` is NOT a Drizzle row variable, even though the file declares a
        // `numeric('amount')` column. Receiver-blind matching would false-fire
        // here; the row-var gate must keep it silent.
        let src = format!("{PRELUDE}\nconst total = parseFloat(form.amount) * 2;");
        assert!(run(&src).is_empty(), "unrelated receiver with a colliding column name must not fire");
    }

    #[test]
    fn ignores_non_db_dot_from_chain() {
        // `dayjs().from(...)` has a `.from` segment but its root is not a db
        // handle and there is no `.select()` — must not mark `order` a row var.
        let src = format!("{PRELUDE}\nconst other = dayjs().from(now);\nconst total = parseFloat(other.amount) * 2;");
        assert!(run(&src).is_empty(), "generic `.from()` on a non-db object must not fire");
    }

    #[test]
    fn ignores_dot_query_property_on_non_db_object() {
        // `router.query` is a Next.js property access, not a Drizzle relational
        // query — its root is not a db handle.
        let src = format!("{PRELUDE}\nconst params = router.query;\nconst total = parseFloat(params.amount) * 2;");
        assert!(run(&src).is_empty(), "`router.query` must not be treated as a row read");
    }

    #[test]
    fn ignores_nested_property_chain() {
        // `order.meta.amount` is a two-hop access — `amount` lives on a nested
        // non-column object, never a Drizzle column read.
        let src = format!("{PRELUDE}\nconst total = parseFloat(order.meta.amount) * 2;");
        assert!(run(&src).is_empty(), "nested `order.meta.amount` is not a column read");
    }

    #[test]
    fn ignores_user_numeric_helper_not_a_table() {
        // `numeric` here is a user object literal, not a Drizzle table column.
        let src = r#"
import { db } from './db';
const cfg = { amount: numeric('x') };
const order = await db.query.orders.findFirst();
const total = parseFloat(order.amount) * 2;
"#;
        assert!(run(src).is_empty(), "no drizzle import / no table constructor → no fire");
    }

    #[test]
    fn ignores_display_formatting_without_arithmetic() {
        // `Number(order.amount)` used only for display, no arithmetic — allowed.
        let src = format!("{PRELUDE}\nconst label = Number(order.amount).toLocaleString();");
        assert!(run(&src).is_empty(), "display-only reparse must not fire");
    }

    #[test]
    fn ignores_parsefloat_not_feeding_arithmetic() {
        // Bare reparse, no arithmetic — display/logging, allowed.
        let src = format!("{PRELUDE}\nconst v = parseFloat(order.amount);");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn skips_test_dirs_via_production_gate() {
        let src = format!("{PRELUDE}\nconst total = parseFloat(order.amount) * qty;");
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, &src, "src/db/__tests__/orders.test.ts")
                .is_empty(),
            "must not fire inside test dirs"
        );
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, &src, "src/db/orders.ts").len(),
            1,
            "must still fire on production paths"
        );
    }
}
