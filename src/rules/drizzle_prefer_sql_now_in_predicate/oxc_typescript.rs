//! drizzle-prefer-sql-now-in-predicate oxc backend.
//!
//! Flag a bare `new Date()` / `Date.now()` (no arguments) used as an argument
//! to a Drizzle filter operator (`eq`/`ne`/`gt`/`gte`/`lt`/`lte`/`between`),
//! e.g. `where(lt(t.expiresAt, new Date()))`. Such code compares a column
//! against the app server's clock, causing app-vs-DB clock skew. The fix is to
//! compare against the database clock with `` sql`now()` `` / `` sql`CURRENT_DATE` ``.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

/// Drizzle filter operators that take a column and a value to compare it against.
const FILTER_OPERATORS: &[&str] = &["eq", "ne", "gt", "gte", "lt", "lte", "between"];

pub struct Check;

/// True when `arg` is a bare `new Date()` (no arguments).
fn is_bare_new_date(arg: &Argument) -> bool {
    let Argument::NewExpression(new_expr) = arg else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    ctor.name.as_str() == "Date" && new_expr.arguments.is_empty()
}

/// True when `arg` is `Date.now()` (no arguments).
fn is_bare_date_now(arg: &Argument) -> bool {
    let Argument::CallExpression(call) = arg else {
        return false;
    };
    if !call.arguments.is_empty() {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "Date" && member.property.name.as_str() == "now"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Date"])
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

        // Callee must be a bare filter-operator identifier (`eq(...)`, `lt(...)`),
        // not an arbitrary `new Date()` elsewhere.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if !FILTER_OPERATORS.contains(&callee.name.as_str()) {
            return;
        }

        // Fire if any argument is a bare `new Date()` / `Date.now()`. The column
        // sits in the first slot; scan every argument so `between(col, a, b)`'s
        // two value slots are both covered.
        let has_bare_clock = call
            .arguments
            .iter()
            .any(|arg| is_bare_new_date(arg) || is_bare_date_now(arg));
        if !has_bare_clock {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}(...)` compares a column against the app server's clock (`new Date()` / `Date.now()`) \u{2014} use `` sql`now()` `` (or `` sql`CURRENT_DATE` ``) so the comparison uses the database clock.",
                callee.name.as_str()
            ),
            severity: Severity::Warning,
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
    fn flags_new_date_in_lt() {
        let src = "db.select().from(t).where(lt(t.expiresAt, new Date()))";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_date_now_in_gte() {
        let src = "db.select().from(t).where(gte(t.createdAt, Date.now()))";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_sql_now() {
        let src = "db.select().from(t).where(lt(t.expiresAt, sql`now()`))";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_date_with_business_date_arg() {
        let src = "db.select().from(t).where(lt(t.expiresAt, new Date(businessDate)))";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_date_with_param_arg() {
        let src = "db.select().from(t).where(gte(t.createdAt, new Date(someParam)))";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_date_with_literal_args() {
        let src = "db.select().from(t).where(lt(t.createdAt, new Date(2024, 0, 1)))";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_new_date_outside_filter_operator() {
        let src = "const d = new Date();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_date_now_outside_filter_operator() {
        let src = "const ts = Date.now();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_new_date_in_between() {
        let src = "db.select().from(t).where(between(t.at, new Date(), endDate))";
        assert_eq!(run(src).len(), 1);
    }
}
