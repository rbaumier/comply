//! sql-no-like-wildcard-prefix — Drizzle ORM backend.
//!
//! Flags Drizzle `like(col, '%...')` / `ilike(col, '%...')` calls where
//! the second argument is a string literal starting with `%`. A leading
//! wildcard prevents the database from using a B-tree index, forcing a
//! sequential scan; full-text search (TSVECTOR + GIN) is the proper fix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(name) = function.utf8_text(source_bytes) else {
            return;
        };
        // Only match simple identifiers (skip member expressions like `obj.like`).
        if name.contains('.') {
            return;
        }
        if name != "like" && name != "ilike" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        // Collect the non-punctuation children of `arguments`.
        let mut cursor = args.walk();
        let arg_nodes: Vec<tree_sitter::Node> = args
            .children(&mut cursor)
            .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
            .collect();
        if arg_nodes.len() < 2 {
            return;
        }
        let second = arg_nodes[1];
        let Ok(second_text) = second.utf8_text(source_bytes) else {
            return;
        };
        if !starts_with_percent_literal(second_text) {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`like(col, '%...')` prevents index usage — use \
                      full-text search instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if the source text of an argument is a string/template literal
/// whose first content character is `%`.
fn starts_with_percent_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    let quote = bytes[0];
    if quote != b'\'' && quote != b'"' && quote != b'`' {
        return false;
    }
    bytes[1] == b'%'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_like_with_leading_wildcard() {
        let src = "db.select().from(users).where(like(users.name, '%john%'));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ilike_with_leading_wildcard() {
        let src = "db.select().from(users).where(ilike(users.email, '%@gmail.com'));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_suffix_wildcard() {
        let src = "db.select().from(users).where(like(users.name, 'john%'));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_eq_call() {
        let src = "db.select().from(users).where(eq(users.name, 'john'));";
        assert!(run(src).is_empty());
    }
}
