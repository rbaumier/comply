//! drizzle-soft-delete-filter oxc backend.
//!
//! In files where `deletedAt` is declared as a column on a Drizzle table
//! schema, flag `.findMany(` or `.select()` call chains that do not
//! include `isNull(` anywhere in the chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const COLUMN_CTORS: &[&str] = &[
    "timestamp",
    "timestamptz",
    "datetime",
    "date",
    "integer",
    "int",
    "bigint",
    "text",
    "varchar",
    "char",
    "boolean",
    "bool",
];

pub struct Check;

/// Check if the source contains a `deletedAt: <columnCtor>(...)` property.
/// Uses source text heuristic: looks for `deletedAt` followed by `:` and
/// then a known column constructor call.
fn file_has_deleted_at_column(source: &str) -> bool {
    // Find all occurrences of "deletedAt" and check what follows.
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find("deletedAt") {
        let abs_pos = search_from + pos;
        let after = &source[abs_pos + "deletedAt".len()..];
        let after = after.trim_start();
        if let Some(rest) = after.strip_prefix(':') {
            let rest = rest.trim_start();
            // Check if it starts with a known column constructor (possibly qualified).
            for ctor in COLUMN_CTORS {
                // Match `timestamp(`, `pg.timestamp(`, `t.timestamp(`, etc.
                if rest.starts_with(ctor) && rest[ctor.len()..].starts_with('(') {
                    return true;
                }
                // Qualified: look for `.ctor(`
                let qualified = format!(".{ctor}(");
                if let Some(dot_pos) = rest.find(&qualified) {
                    // Ensure the part before the dot is an identifier-like string.
                    if dot_pos < 30 {
                        let before = &rest[..dot_pos];
                        if before.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            return true;
                        }
                    }
                }
            }
        }
        search_from = abs_pos + 1;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["deletedAt"])
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

        if !ctx.source_contains("deletedAt") {
            return;
        }

        // `file_has_deleted_at_column` scans the whole source; it's a file-level
        // property, so memoize it once per file instead of rescanning on every
        // CallExpression node (the per-node call made this O(nodes × source)).
        if !crate::oxc_helpers::cached_file_bool(
            ctx.source,
            crate::oxc_helpers::SLOT_DELETED_AT_COLUMN,
            || file_has_deleted_at_column(ctx.source),
        ) {
            return;
        }

        // Callee must be `*.findMany` or `*.select`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let name = member.property.name.as_str();
        if name != "findMany" && name != "select" {
            return;
        }

        // Walk up the call chain via source text and check for `isNull(`.
        let _chain_text = &ctx.source[call.span.start as usize..];
        // Find the extent of the chain: we need the full chain from the
        // outermost expression. Use a simpler approach: check the whole
        // line/surrounding source for `isNull(`.
        // Actually, use the span from the root of the member chain.
        let chain_start = find_chain_start(&member.object, ctx.source);
        let chain_source = &ctx.source[chain_start..call.span.end as usize];
        if chain_source.contains("isNull(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{name}(...)` on a soft-deletable table without `isNull(t.deletedAt)` \u{2014} add the filter or use a dedicated non-deleted helper."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk down the member expression chain to find the start of the chain.
fn find_chain_start(expr: &Expression, _source: &str) -> usize {
    match expr {
        Expression::StaticMemberExpression(m) => find_chain_start(&m.object, _source),
        Expression::ComputedMemberExpression(m) => find_chain_start(&m.object, _source),
        Expression::CallExpression(c) => c.span.start as usize,
        _ => {
            use oxc_span::GetSpan;
            expr.span().start as usize
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_findmany_without_isnull() {
        let src = "export const users = pgTable('u', { id: text('id'), deletedAt: timestamp('deleted_at') });\n\
                   const r = db.query.users.findMany({})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findmany_with_isnull() {
        let src = "export const users = pgTable('u', { id: text('id'), deletedAt: timestamp('deleted_at') });\n\
                   const r = db.query.users.findMany({ where: isNull(users.deletedAt) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_deleted_at() {
        let src = "const r = db.query.users.findMany({})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_bare_deleted_at_mention_without_column_definition() {
        let src = "const t = { deletedAt };\nconst r = db.query.users.findMany({})";
        assert!(run(src).is_empty());
    }
}
