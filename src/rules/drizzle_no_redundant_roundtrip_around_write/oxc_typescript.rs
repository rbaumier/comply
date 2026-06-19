//! drizzle-no-redundant-roundtrip-around-write oxc backend.
//!
//! Within one block scope this rule flags two redundant non-atomic round-trips:
//!
//! (a) a Drizzle read-probe (`db.query.T.findFirst({ where })` /
//!     `db.select().from(T).where(...)`) whose result is only consulted in a
//!     truthiness branch that diverts (returns/throws) when the row is present
//!     or absent, guarding a later write (`db.insert(T)` / `db.update(T)` /
//!     `db.delete(T)`) on the **same table + same key**; the probe should be
//!     folded into the write via `.onConflictDoNothing()` /
//!     `.onConflictDoUpdate()`.
//!
//! (b) a write on `T` followed within the configured statement gap by a read on
//!     `T` keyed on the column the write produced — the row should have been
//!     read back with `.returning()` instead of a second query.
//!
//! Both halves are scoped to the statement slice of one block: two operations
//! in unrelated scopes never run in sequence. The probe/write must share the
//! same table and the same key column(s). A `where` with more than one
//! `eq(T.col, …)` filter (a composite key) is not paired — the column set
//! cannot be proven equal cheaply, so the rule stays silent. Re-reads that pull
//! extra `with:` relations a write cannot return are not reported.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, Statement, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// A Drizzle read-probe or write extracted from a single statement.
#[derive(Debug)]
struct DbOp {
    kind: OpKind,
    table: String,
    /// The `eq(table.col, …)` filter column when the statement carries exactly
    /// one such key filter. A `where` with zero or more than one `eq(table.…)`
    /// is `None`: a composite key cannot be proven equal cheaply, and the
    /// conservative stance is to not pair such statements.
    key_col: Option<String>,
    /// Columns written by an `insert(...).values({ col: … })` literal. Used by
    /// pattern (b) to confirm a re-read keys on a just-written column.
    value_cols: Vec<String>,
    /// True for an `insert` write (no `where` key). An insert with a single-key
    /// probe is the canonical upsert pairing; an `update`/`delete` whose `where`
    /// is composite (so its `key_col` is `None`) must not be paired on table
    /// alone — the probe would prove only part of its key.
    is_insert: bool,
    /// The `const`/`let` binding the read result is assigned to (reads only).
    binding: Option<String>,
    /// A read that pulls `with:` relations cannot be replaced by `.returning()`.
    has_with: bool,
    /// A write that already reads its row back via `.returning()`.
    has_returning: bool,
    /// A write that already folds the existence check via `.onConflict…()`.
    has_on_conflict: bool,
    /// Byte offset of the statement, for the diagnostic position.
    offset: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum OpKind {
    Read,
    Write,
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Both halves need a write; every write is an insert/update/delete.
        Some(&[".insert(", ".update(", ".delete("])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max_gap = ctx.config.threshold(
            "drizzle-no-redundant-roundtrip-around-write",
            "max_statement_gap",
            ctx.lang,
        );

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let stmts: &[Statement] = match node.kind() {
                AstKind::Program(prog) => &prog.body,
                AstKind::FunctionBody(body) => &body.statements,
                AstKind::BlockStatement(block) => &block.body,
                _ => continue,
            };
            scan_block(stmts, ctx.source, max_gap, &ctx.path_arc, &mut diagnostics);
        }
        diagnostics
    }
}

/// Scan one statement slice for the two redundant-roundtrip patterns. Only
/// direct statements of this block are paired — a probe and write that live in
/// different scopes never execute in a single check-then-act sequence.
fn scan_block(
    stmts: &[Statement],
    source: &str,
    max_gap: usize,
    path_arc: &Arc<std::path::Path>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ops: Vec<Option<DbOp>> = stmts.iter().map(|s| extract_op(s, source)).collect();

    // Pattern (a): a keyed read whose binding gates a same-table+same-key write
    // through a diverting truthiness branch.
    for (i, read) in ops.iter().enumerate().filter_map(|(i, o)| Some((i, read_op(o)?))) {
        let Some(binding) = &read.binding else { continue };
        if read.key_col.is_none() {
            continue;
        }
        if let Some(write) = find_guarded_write(stmts, &ops, i, binding, read, max_gap) {
            push_diag(diagnostics, path_arc, source, write.offset, Pattern::PreCheck, &read.table);
        }
    }

    // Pattern (b): a write without `.returning()` followed within the gap by a
    // keyed read on the same table whose key the write demonstrably produced.
    for (i, write) in ops.iter().enumerate().filter_map(|(i, o)| Some((i, write_op(o)?))) {
        if write.has_returning {
            continue;
        }
        let upper = (i + 1 + max_gap).min(ops.len());
        for read in ops[i + 1..upper].iter().filter_map(read_op) {
            if read.table == write.table && !read.has_with && reread_keys_written(read, write) {
                push_diag(diagnostics, path_arc, source, read.offset, Pattern::PostReread, &write.table);
                break;
            }
        }
    }
}

fn read_op(op: &Option<DbOp>) -> Option<&DbOp> {
    op.as_ref().filter(|o| o.kind == OpKind::Read)
}

fn write_op(op: &Option<DbOp>) -> Option<&DbOp> {
    op.as_ref().filter(|o| o.kind == OpKind::Write)
}

/// Pattern (a): starting after the read at `read_idx`, find a write on the same
/// table+key whose execution is gated by a diverting truthiness branch on
/// `binding`. A guard is an `if` whose test is the probe binding's truthiness
/// (`if (x)`, `if (!x)`, `if (x == null)`, …) and whose taken branch diverts
/// with `return`/`throw` — so the write runs only on the complementary path.
fn find_guarded_write<'a>(
    stmts: &[Statement],
    ops: &'a [Option<DbOp>],
    read_idx: usize,
    binding: &str,
    read: &DbOp,
    max_gap: usize,
) -> Option<&'a DbOp> {
    let upper = (read_idx + 1 + max_gap).min(ops.len());
    let mut guard_seen = false;
    for j in read_idx + 1..upper {
        if is_diverting_guard_on(&stmts[j], binding) {
            guard_seen = true;
        }
        if let Some(write) = write_op(&ops[j]) {
            if guard_seen
                && write.table == read.table
                && !write.has_on_conflict
                && keys_match(read, write)
            {
                return Some(write);
            }
            return None;
        }
    }
    None
}

/// The probe and the write target the same key when both expose exactly one
/// `eq(table.col, …)` column and the columns are equal. When the write is an
/// `insert` (no readable where-key), the gating branch plus the table match
/// stand: an insert's conflict target is not recoverable from the AST, and the
/// `if (existing) …` guard already establishes the intent. An `update`/`delete`
/// whose key is `None` is composite (multiple `eq`) — the single-key probe
/// proves only part of its key, so it is not paired.
fn keys_match(read: &DbOp, write: &DbOp) -> bool {
    match (&read.key_col, &write.key_col) {
        (Some(rc), Some(wc)) => rc == wc,
        (Some(_), None) => write.is_insert,
        // A read without a recoverable single key never reaches here — pattern
        // (a) requires `read.key_col.is_some()`.
        (None, _) => false,
    }
}

/// Pattern (b): the re-read keys on a column the write demonstrably produced.
/// For an `update`/`delete` that means the same `eq(table.col, …)` column; for
/// an `insert` it means the re-read's key column appears in the inserted
/// `values({ … })` literal. An insert without a recoverable value set cannot
/// establish the re-read targets the just-written row, so it is not paired.
fn reread_keys_written(read: &DbOp, write: &DbOp) -> bool {
    let Some(read_col) = &read.key_col else { return false };
    if let Some(write_col) = &write.key_col {
        return read_col == write_col;
    }
    write.value_cols.iter().any(|c| c == read_col)
}

enum Pattern {
    PreCheck,
    PostReread,
}

fn push_diag(
    diagnostics: &mut Vec<Diagnostic>,
    path_arc: &Arc<std::path::Path>,
    source: &str,
    offset: usize,
    pattern: Pattern,
    table: &str,
) {
    let message = match pattern {
        Pattern::PreCheck => format!(
            "Existence-probe guarding a write on `{table}` is a redundant non-atomic round-trip — fold it into the write with `.onConflictDoNothing()` / `.onConflictDoUpdate()`."
        ),
        Pattern::PostReread => format!(
            "Re-reading `{table}` right after writing it is a redundant round-trip — read the row back with `.returning()` instead."
        ),
    };
    let (line, column) = byte_offset_to_line_col(source, offset);
    diagnostics.push(Diagnostic {
        path: Arc::clone(path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Warning,
        span: None,
    });
}

/// The source text of a statement.
fn stmt_text<'a>(stmt: &Statement, source: &'a str) -> &'a str {
    let span = stmt.span();
    &source[span.start as usize..span.end as usize]
}

/// True when `stmt` is an `if` whose test is a truthiness check on `binding`
/// (`x`, `!x`, `x == null`, `x === undefined`, …) and whose taken branch
/// diverts with `return`/`throw`. Such a branch is what makes the following
/// write conditional on the probe result.
fn is_diverting_guard_on(stmt: &Statement, binding: &str) -> bool {
    let Statement::IfStatement(if_stmt) = stmt else { return false };
    if !test_is_truthiness_of(&if_stmt.test, binding) {
        return false;
    }
    branch_diverts(&if_stmt.consequent)
}

/// True when `expr` is a truthiness/nullishness test whose sole operand is
/// `binding`: `x`, `!x`, or a comparison of `x` against `null`/`undefined`.
fn test_is_truthiness_of(expr: &Expression, binding: &str) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == binding,
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
            test_is_truthiness_of(&unary.argument, binding)
        }
        Expression::BinaryExpression(bin) => {
            let is_eq = matches!(
                bin.operator,
                BinaryOperator::Equality
                    | BinaryOperator::Inequality
                    | BinaryOperator::StrictEquality
                    | BinaryOperator::StrictInequality
            );
            is_eq
                && ((is_identifier(&bin.left, binding) && is_null_or_undefined(&bin.right))
                    || (is_identifier(&bin.right, binding) && is_null_or_undefined(&bin.left)))
        }
        _ => false,
    }
}

fn is_identifier(expr: &Expression, name: &str) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == name)
}

fn is_null_or_undefined(expr: &Expression) -> bool {
    matches!(expr, Expression::NullLiteral(_))
        || matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// True when the statement (or the first statement of its block) is a `return`
/// or `throw` — the branch diverts control away from the following write.
fn branch_diverts(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
        Statement::BlockStatement(block) => block.body.iter().any(branch_diverts),
        _ => false,
    }
}

/// Extract a [`DbOp`] from a single statement, or `None` when it is not a
/// recognised Drizzle read-probe or write.
fn extract_op(stmt: &Statement, source: &str) -> Option<DbOp> {
    let text = stmt_text(stmt, source);
    let offset = stmt.span().start as usize;

    if let Some((table, key_col, has_with)) = extract_read(text) {
        return Some(DbOp {
            kind: OpKind::Read,
            table,
            key_col,
            value_cols: Vec::new(),
            is_insert: false,
            binding: declared_binding(stmt, source),
            has_with,
            has_returning: false,
            has_on_conflict: false,
            offset,
        });
    }
    if let Some(write) = extract_write(text) {
        return Some(DbOp {
            kind: OpKind::Write,
            table: write.table,
            key_col: write.key_col,
            value_cols: write.value_cols,
            is_insert: write.is_insert,
            binding: None,
            has_with: false,
            has_returning: text.contains(".returning("),
            has_on_conflict: text.contains(".onConflict"),
            offset,
        });
    }
    None
}

/// The `const`/`let`/`var NAME = …` binding name of a statement, if it is a
/// single plain-identifier declaration. A destructuring pattern is not a simple
/// truthiness handle and yields `None`.
fn declared_binding(stmt: &Statement, source: &str) -> Option<String> {
    let Statement::VariableDeclaration(decl) = stmt else { return None };
    if decl.declarations.len() != 1 {
        return None;
    }
    let d = decl.declarations.first()?;
    let span = d.id.span();
    let ident = source[span.start as usize..span.end as usize].trim();
    if ident.is_empty() || !ident.chars().all(is_ident_char) {
        return None;
    }
    Some(ident.to_string())
}

/// Recognise a read-probe and return `(table, key_col, has_with)`.
///
/// Two shapes are recognised:
///   - `<db>.query.<table>.findFirst({ where: eq(<table>.<col>, …), with?: … })`
///   - `<db>.select(…).from(<table>).where(eq(<table>.<col>, …))`
fn extract_read(text: &str) -> Option<(String, Option<String>, bool)> {
    if text.contains(".findFirst(") {
        let table = table_after(text, ".query.")?;
        let key_col = single_eq_column(text, &table);
        let has_with = text.contains("with:") || text.contains("with :");
        return Some((table, key_col, has_with));
    }
    if text.contains(".select(") && text.contains(".from(") {
        let table = call_arg_table(text, ".from(")?;
        let key_col = single_eq_column(text, &table);
        // `.select()` cannot carry `with:` relations; only the relational query
        // API does. Shape-wise always replaceable by `.returning()`.
        return Some((table, key_col, false));
    }
    None
}

/// The table-targeting shape of a write statement.
struct WriteShape {
    table: String,
    /// Single `eq(table.col, …)` column of an `update`/`delete` `where`.
    key_col: Option<String>,
    /// Column set of an `insert(...).values({ … })` literal.
    value_cols: Vec<String>,
    is_insert: bool,
}

/// Recognise a write (`insert` / `update` / `delete`) and its key shape.
fn extract_write(text: &str) -> Option<WriteShape> {
    if let Some(table) = call_arg_table(text, ".insert(") {
        return Some(WriteShape {
            table,
            key_col: None,
            value_cols: insert_value_columns(text),
            is_insert: true,
        });
    }
    for method in [".update(", ".delete("] {
        if let Some(table) = call_arg_table(text, method) {
            return Some(WriteShape {
                key_col: single_eq_column(text, &table),
                table,
                value_cols: Vec::new(),
                is_insert: false,
            });
        }
    }
    None
}

/// The single identifier argument of the call opened by `marker`
/// (e.g. `.from(users)` → `users`, `.insert(orders)` → `orders`).
/// Returns `None` when the argument is not a bare identifier (e.g.
/// `.from(schema.users)` or `.insert(getTable())`).
fn call_arg_table(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let arg: String = rest.chars().take_while(|&c| is_ident_char(c)).collect();
    if arg.is_empty() {
        return None;
    }
    let after = &rest[arg.len()..];
    if !after.trim_start().starts_with(')') {
        return None;
    }
    Some(arg)
}

/// The table name following `.query.` (`<db>.query.<table>.findFirst`).
fn table_after(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let name: String = rest.chars().take_while(|&c| is_ident_char(c)).collect();
    if name.is_empty() { None } else { Some(name) }
}

/// The column of the `eq(<table>.<col>, …)` filter in `text` **only when there
/// is exactly one** such filter. Anchoring on `<table>.` keeps a `where` keyed
/// on a different table out of the comparison; requiring a single occurrence
/// keeps composite keys (`and(eq(t.a, …), eq(t.b, …))`) unpaired, since the
/// column set cannot be proven equal cheaply.
fn single_eq_column(text: &str, table: &str) -> Option<String> {
    let needle = format!("eq({table}.");
    let mut matches = text.match_indices(&needle);
    let (first, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let rest = &text[first + needle.len()..];
    let col: String = rest.chars().take_while(|&c| is_ident_char(c)).collect();
    if col.is_empty() { None } else { Some(col) }
}

/// The column keys of the first `.values({ … })` object literal in an insert
/// statement, covering both shorthand (`{ id, name }`) and explicit
/// (`{ id: x, name: y }`) properties. A key sits at the object's start or right
/// after a top-level comma; the first top-level identifier there is the column,
/// and an explicit `key: value` skips its value to the next comma. Used only to
/// confirm a pattern-(b) re-read keys on a written column, so over-capturing is
/// harmless and under-capturing only suppresses.
fn insert_value_columns(text: &str) -> Vec<String> {
    let Some(start) = text.find(".values(") else { return Vec::new() };
    let rest = &text[start + ".values(".len()..];
    let Some(brace) = rest.find('{') else { return Vec::new() };
    let body = &rest[brace + 1..];
    let bytes = body.as_bytes();
    let mut cols = Vec::new();
    let mut depth = 0i32;
    let mut at_key = true;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' if depth == 0 => break,
            b'}' | b']' | b')' => depth -= 1,
            b',' if depth == 0 => at_key = true,
            _ if depth == 0 && at_key && is_ident_byte(bytes[i]) => {
                let begin = i;
                while i < bytes.len() && is_ident_byte(bytes[i]) {
                    i += 1;
                }
                cols.push(body[begin..i].to_string());
                at_key = false;
                continue;
            }
            _ if depth == 0 && !bytes[i].is_ascii_whitespace() => at_key = false,
            _ => {}
        }
        i += 1;
    }
    cols
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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

    // ---- Pattern (a): existence pre-check before a write -------------------

    // The issue's bad example: probe, diverting guard, then insert on the same table.
    #[test]
    fn flags_existence_check_before_insert() {
        let src = r#"
            async function create(db, k) {
              const x = await db.query.t.findFirst({ where: eq(t.k, k) });
              if (x) throw new Conflict();
              await db.insert(t).values({ k });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_existence_check_before_update_same_key() {
        let src = r#"
            async function upd(db, id, name) {
              const existing = await db.query.users.findFirst({ where: eq(users.id, id) });
              if (!existing) return;
              await db.update(users).set({ name }).where(eq(users.id, id));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_existence_check_with_null_compare_guard() {
        let src = r#"
            async function f(db, id) {
              const existing = await db.query.users.findFirst({ where: eq(users.id, id) });
              if (existing != null) throw new Conflict();
              await db.insert(users).values({ id });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // The issue's good example: the probe is folded into the write.
    #[test]
    fn allows_on_conflict_do_nothing() {
        let src = r#"
            async function create(db, k) {
              await db.insert(t).values({ k }).onConflictDoNothing();
            }
        "#;
        assert!(run(src).is_empty());
    }

    // ---- Pattern (b): re-read after a write --------------------------------

    #[test]
    fn flags_reread_after_insert() {
        let src = r#"
            async function create(db, id, name) {
              await db.insert(users).values({ id, name });
              const created = await db.query.users.findFirst({ where: eq(users.id, id) });
              return created;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_returning_instead_of_reread() {
        let src = r#"
            async function create(db, id, name) {
              const [created] = await db.insert(users).values({ id, name }).returning();
              return created;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // ---- Guardrail negatives ----------------------------------------------

    // Probe keyed on a different column than the write — not the same key.
    #[test]
    fn no_fp_different_key_column() {
        let src = r#"
            async function f(db, email, id, name) {
              const existing = await db.query.users.findFirst({ where: eq(users.email, email) });
              if (existing) throw new Conflict();
              await db.update(users).set({ name }).where(eq(users.id, id));
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Re-read pulls a `with:` relation the write cannot return — not redundant.
    #[test]
    fn no_fp_reread_with_relation() {
        let src = r#"
            async function create(db, id, name) {
              await db.insert(users).values({ id, name });
              const created = await db.query.users.findFirst({ where: eq(users.id, id), with: { posts: true } });
              return created;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Probe and write on different tables — never paired.
    #[test]
    fn no_fp_different_tables() {
        let src = r#"
            async function f(db, id) {
              const existing = await db.query.users.findFirst({ where: eq(users.id, id) });
              if (existing) throw new Conflict();
              await db.insert(orders).values({ userId: id });
            }
        "#;
        assert!(run(src).is_empty());
    }

    // A probe never consulted in a guard before the write is not a check-then-act.
    #[test]
    fn no_fp_probe_not_used_as_guard() {
        let src = r#"
            async function f(db, id, name) {
              const current = await db.query.users.findFirst({ where: eq(users.id, id) });
              await db.update(users).set({ name }).where(eq(users.id, id));
              return current;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Probe and write in unrelated scopes never run in sequence.
    #[test]
    fn no_fp_probe_and_write_in_separate_functions() {
        let src = r#"
            async function read(db, id) {
              const u = await db.query.users.findFirst({ where: eq(users.id, id) });
              if (u) return u;
            }
            async function write(db, id) {
              await db.insert(users).values({ id });
            }
        "#;
        assert!(run(src).is_empty());
    }

    // A write far past the configured statement gap from its re-read is not paired.
    #[test]
    fn no_fp_reread_beyond_gap() {
        let src = r#"
            async function f(db, id) {
              await db.insert(users).values({ id });
              doSomething();
              doSomethingElse();
              andMore();
              const created = await db.query.users.findFirst({ where: eq(users.id, id) });
              return created;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for review finding #1: the probe binding only appears in the
    // `if` BODY (logging), the write runs unconditionally — not a guard.
    #[test]
    fn no_fp_binding_only_in_if_body() {
        let src = r#"
            async function f(db, k) {
              const x = await db.query.t.findFirst({ where: eq(t.k, k) });
              if (x) { logger.info(x); }
              await db.insert(t).values({ k });
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for review finding #1: the divert is gated on an unrelated flag;
    // the probe binding is not the tested condition.
    #[test]
    fn no_fp_guard_on_unrelated_flag() {
        let src = r#"
            async function f(db, id, name, someOtherFlag) {
              const existing = await db.query.users.findFirst({ where: eq(users.id, id) });
              if (someOtherFlag) { console.log(existing); return; }
              await db.update(users).set({ name }).where(eq(users.id, id));
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for review finding #1: the `if` tests the binding but does NOT
    // divert (no return/throw), so the write still runs unconditionally.
    #[test]
    fn no_fp_guard_without_divert() {
        let src = r#"
            async function f(db, id, name) {
              const existing = await db.query.users.findFirst({ where: eq(users.id, id) });
              if (existing) { name = existing.name; }
              await db.update(users).set({ name }).where(eq(users.id, id));
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for review finding #2: insert then re-read a DIFFERENT id
    // (column written, but a distinct value lookup) — still flagged only when
    // the key column is the written one. Here the re-read keys on `email`, a
    // column never written by the insert, so it is a legitimate lookup.
    #[test]
    fn no_fp_reread_different_column() {
        let src = r#"
            async function f(db, id, title, slug) {
              await db.insert(posts).values({ id, title });
              const bySlug = await db.query.posts.findFirst({ where: eq(posts.slug, slug) });
              return bySlug;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for review finding #3: a composite-key probe is not paired,
    // because the column set cannot be proven equal to the write's key cheaply.
    #[test]
    fn no_fp_composite_key_probe() {
        let src = r#"
            async function f(db, a, b, name) {
              const existing = await db.query.t.findFirst({ where: and(eq(t.a, a), eq(t.b, b)) });
              if (existing) throw new Conflict();
              await db.update(t).set({ name }).where(eq(t.a, a));
            }
        "#;
        assert!(run(src).is_empty());
    }

    // A single-key probe before a COMPOSITE-key update is not paired: the probe
    // proves only `id`, but the update targets `id + tenant`, so it is not a
    // proven same-row pairing.
    #[test]
    fn no_fp_single_probe_before_composite_update() {
        let src = r#"
            async function f(db, id, tenant, name) {
              const existing = await db.query.t.findFirst({ where: eq(t.id, id) });
              if (!existing) return;
              await db.update(t).set({ name }).where(and(eq(t.id, id), eq(t.tenant, tenant)));
            }
        "#;
        assert!(run(src).is_empty());
    }

    // The rule is suppressed in test directories.
    #[test]
    fn gated_no_fp_in_test_directory() {
        let src = r#"
            async function create(db, k) {
              const x = await db.query.t.findFirst({ where: eq(t.k, k) });
              if (x) throw new Conflict();
              await db.insert(t).values({ k });
            }
        "#;
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "tests/create.test.ts").is_empty(),
            "skip_in_test_dir must suppress the rule for test-directory files"
        );
    }

    // Non-Drizzle code is never flagged.
    #[test]
    fn no_fp_non_drizzle() {
        let src = r#"
            function f(cache, id) {
              const hit = cache.get(id);
              if (hit) return hit;
              cache.set(id, compute(id));
            }
        "#;
        assert!(run(src).is_empty());
    }
}
