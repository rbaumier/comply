//! OxcCheck backend for drizzle-no-select-without-limit.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

/// Largest index `<= idx` that lies on a UTF-8 char boundary. A fixed-size byte
/// window can otherwise land inside a multi-byte char (e.g. an em-dash in a
/// comment) and panic `&str` slicing.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Check if a call expression is part of a `.select().from()` chain
/// without `.limit()` or `.where()`.
fn check_select_chain(call: &oxc_ast::ast::CallExpression, source: &str) -> Option<u32> {
    // This call must be `.select(...)`
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    if member.property.name.as_str() != "select" {
        return None;
    }

    // Now we need to check if there's a `.from()` in the chain above us,
    // but in oxc's AST the chain is inverted — we ARE the inner call.
    // The outer calls wrap US. We can't walk up without semantic parent info.
    // So instead, we look at the source text starting from our position
    // to find the chain.

    // Alternative approach: scan a wider window of source after our span
    // to detect `.from(`, `.limit(`, `.where(` in the chain.
    let start = call.span.start as usize;
    // Look at a reasonable window after the select call. Clamp the end to a
    // char boundary so a multi-byte char straddling the window edge doesn't
    // panic the slice.
    let window_end = floor_char_boundary(source, (start + 500).min(source.len()));
    let window = &source[start..window_end];

    // Find end of the expression statement (semicolon, newline after last paren, etc.)
    let mut depth = 0i32;
    let mut expr_end = window.len();
    let bytes = window.as_bytes();
    let mut past_select = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => {
                depth += 1;
                past_select = true;
            }
            b')' => {
                depth -= 1;
                if past_select && depth == 0 {
                    // Check what follows
                    if i + 1 < bytes.len() && bytes[i + 1] == b'.' {
                        // More chaining, continue
                    } else {
                        expr_end = i + 1;
                        break;
                    }
                }
            }
            b';' | b'\n' if depth <= 0 => {
                expr_end = i;
                break;
            }
            _ => {}
        }
    }

    let chain_text = &window[..expr_end];
    let has_from = chain_text.contains(".from(");
    let has_limit = chain_text.contains(".limit(");
    let has_where = chain_text.contains(".where(");

    if has_from && !has_limit && !has_where {
        if is_insert_returning_cte_select(call, chain_text, source, start) {
            return None;
        }
        Some(call.span.start)
    } else {
        None
    }
}

/// Extracts a simple identifier from a `.from(X)` call in `chain_text`.
fn extract_from_identifier(chain_text: &str) -> Option<&str> {
    let from_pos = chain_text.find(".from(")?;
    let arg_start = from_pos + ".from(".len();
    let remaining = &chain_text[arg_start..];
    let arg_end = remaining.find(|c: char| c != '_' && c != '$' && !c.is_alphanumeric())?;
    if arg_end == 0 {
        return None;
    }
    Some(&remaining[..arg_end])
}

/// Returns true when `var_name` is declared as a `$with(...).as(insert(...).returning())`
/// CTE somewhere in `source` before byte `before_pos`.
fn is_insert_returning_definition(var_name: &str, source: &str, before_pos: usize) -> bool {
    let preceding = &source[..before_pos.min(source.len())];
    let mut pos = 0;
    while pos < preceding.len() {
        let Some(idx) = preceding[pos..].find(var_name) else { break };
        let abs = pos + idx;
        let rest = &preceding[abs + var_name.len()..];
        let trimmed = rest.trim_start_matches(|c: char| c == ' ' || c == '\t');
        if trimmed.starts_with('=') && !trimmed.starts_with("==") && !trimmed.starts_with("=>") {
            let end = (abs + 1000).min(preceding.len());
            let window = &preceding[abs..end];
            if window.contains("$with(") && window.contains(".returning()") {
                return true;
            }
        }
        pos = abs + 1;
    }
    false
}

/// Returns true when the `.select()` chain reads from a CTE that is defined as
/// INSERT...RETURNING — meaning the result is structurally bounded to ≤ 1 row.
fn is_insert_returning_cte_select(
    call: &oxc_ast::ast::CallExpression,
    chain_text: &str,
    source: &str,
    span_start: usize,
) -> bool {
    // The receiver of .select() must be a .with(...) call
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let Expression::CallExpression(with_call) = &member.object else { return false };
    let Expression::StaticMemberExpression(with_member) = &with_call.callee else { return false };
    if with_member.property.name.as_str() != "with" {
        return false;
    }

    // Collect CTE variable names from .with(a, b, ...)
    let cte_names: Vec<&str> = with_call
        .arguments
        .iter()
        .filter_map(|arg| {
            if let Argument::Identifier(ident) = arg {
                Some(ident.name.as_str())
            } else {
                None
            }
        })
        .collect();

    if cte_names.is_empty() {
        return false;
    }

    // The .from(X) argument must be one of the .with() CTE variables
    let Some(from_arg) = extract_from_identifier(chain_text) else { return false };
    if !cte_names.contains(&from_arg) {
        return false;
    }

    // Confirm the CTE variable is defined as INSERT...RETURNING
    is_insert_returning_definition(from_arg, source, span_start)
}

/// True when the receiver chain of a `.findMany()` call goes through a `.query.`
/// member — the Drizzle relational API shape `db.query.<table>.findMany(...)`.
/// This excludes generic `.findMany()` calls from other libraries.
fn receiver_is_drizzle_query(mut expr: &Expression) -> bool {
    while let Expression::StaticMemberExpression(member) = expr {
        if member.property.name.as_str() == "query" {
            return true;
        }
        expr = &member.object;
    }
    false
}

/// True when an argument object carries a `limit:` or `where:` property — either
/// bounds the relational query, so it must not be flagged.
fn findmany_arg_is_bounded(expr: &Expression) -> bool {
    let Expression::ObjectExpression(obj) = expr else { return false };
    obj.properties.iter().any(|prop| {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            p.key.name().is_some_and(|n| n == "limit" || n == "where")
        } else {
            false
        }
    })
}

/// Check if a call expression is a `db.query.<table>.findMany(arg)` whose
/// argument object has neither a `limit` nor a `where` property. Returns the
/// byte offset of the `findMany` property for the diagnostic.
fn check_findmany(call: &oxc_ast::ast::CallExpression) -> Option<u32> {
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    if member.property.name.as_str() != "findMany" {
        return None;
    }
    if !receiver_is_drizzle_query(&member.object) {
        return None;
    }
    let bounded = call
        .arguments
        .iter()
        .filter_map(|arg| arg.as_expression())
        .any(findmany_arg_is_bounded);
    if bounded {
        return None;
    }
    Some(member.property.span.start)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["select", "findMany"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if let Some(span_start) = check_select_chain(call, ctx.source) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`db.select().from(table)` without `.limit()` or `.where()` scans the \
                          entire table — add a bound to avoid loading unbounded rows."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        if let Some(span_start) = check_findmany(call) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`db.query.<table>.findMany()` without `limit` or `where` scans the \
                          entire table — add a bound to avoid loading unbounded rows."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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
    fn flags_select_from_without_bound() {
        assert_eq!(run("db.select({ x: 1 }).from(t);").len(), 1);
    }

    #[test]
    fn allows_select_with_limit() {
        assert!(run("db.select({ x: 1 }).from(t).limit(10);").is_empty());
    }

    #[test]
    fn floor_char_boundary_walks_back_into_multibyte() {
        let s = "ab—cd"; // em-dash is bytes 2..5
        assert_eq!(floor_char_boundary(s, 3), 2);
        assert_eq!(floor_char_boundary(s, 4), 2);
        assert_eq!(floor_char_boundary(s, 5), 5);
        assert_eq!(floor_char_boundary(s, 999), s.len());
    }

    // Regression for #532: SELECT from an INSERT RETURNING CTE is bounded to <= 1 row.
    #[test]
    fn allows_select_from_insert_returning_cte() {
        let src = r#"
const insertedUser = tx.$with('inserted_user').as(
  tx.insert(user).values({ email: 'test@test.com' }).returning()
);
const rows = await tx.with(insertedUser).select().from(insertedUser);
"#;
        assert!(run(src).is_empty(), "should not flag CTE from INSERT RETURNING");
    }

    #[test]
    fn flags_select_from_regular_table_even_with_cte_in_scope() {
        // .with() is present but .from() uses a table schema, not the CTE variable
        let src = r#"
const insertedUser = tx.$with('inserted_user').as(
  tx.insert(user).values({ email: 'test@test.com' }).returning()
);
const rows = await tx.with(insertedUser).select().from(usersTable);
"#;
        assert_eq!(run(src).len(), 1, "should flag when .from() is not the INSERT RETURNING CTE");
    }

    // Regression for #979: integration/type tests intentionally read whole
    // tables — the rule is gated out of test dirs, including `*-tests/`.
    #[test]
    fn skips_test_dirs_via_production_gate() {
        let src = "const r = await db.select().from(users);";
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "integration-tests/type-tests/join-nodenext/mysql.ts"
            )
            .is_empty(),
            "must not fire inside *-tests/ dirs"
        );
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "drizzle-arktype/tests/pg.test.ts")
                .is_empty(),
            "must not fire inside tests/ dirs"
        );
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/db/queries.ts").len(),
            1,
            "must still fire on production paths"
        );
    }

    // Regression for #265: an em-dash straddling the 500-byte scan window must
    // not panic the slice. Padded so byte 500 lands inside the em-dash.
    #[test]
    fn does_not_panic_on_multibyte_at_window_edge() {
        let mut src = String::from("db.select({ x: 1 }).from(t); //");
        while src.len() < 499 {
            src.push('x');
        }
        src.push('—'); // occupies bytes 499..502 — byte 500 is mid-char
        let diags = run(&src);
        assert_eq!(diags.len(), 1, "should flag the unbounded select, not panic");
    }

    // Issue #3762: the relational API `db.query.<table>.findMany()` without a
    // `limit`/`where` bound is the same unbounded-scan hazard as a bare select.
    #[test]
    fn flags_findmany_without_args() {
        assert_eq!(run("await db.query.users.findMany();").len(), 1);
    }

    #[test]
    fn flags_findmany_with_object_lacking_limit_and_where() {
        assert_eq!(run("db.query.users.findMany({ orderBy: x });").len(), 1);
    }

    #[test]
    fn allows_findmany_with_limit() {
        assert!(run("db.query.users.findMany({ limit: 100 });").is_empty());
    }

    #[test]
    fn allows_findmany_with_where() {
        assert!(run("db.query.users.findMany({ where: eq(users.id, 1) });").is_empty());
    }

    // A bare `.findMany()` not routed through the Drizzle `.query.` relational
    // API must not be flagged — it is a generic call from another library.
    #[test]
    fn ignores_findmany_without_query_receiver() {
        assert!(run("repo.users.findMany();").is_empty());
    }

    // Prefilter coverage: the SIMD prefilter prunes files that contain none of
    // these tokens before the check runs, so it MUST list `findMany` or a
    // findMany-only file (no `select`) would be skipped entirely.
    #[test]
    fn prefilter_covers_findmany_trigger() {
        let tokens = Check.prefilter().expect("rule defines a prefilter");
        assert!(tokens.contains(&"findMany"), "prefilter must include the findMany trigger");
        assert!(tokens.contains(&"select"), "prefilter must still include the select trigger");
    }

    // Behavioral counterpart: a source whose only Drizzle call is `findMany()`
    // (no `select` token) still fires the check on a production path.
    #[test]
    fn findmany_only_source_fires() {
        let src = "const rows = await db.query.posts.findMany();";
        assert!(!src.contains("select"), "fixture has no select token");
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/db/queries.ts").len(),
            1,
            "findMany-only file must fire"
        );
    }
}
