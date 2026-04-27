//! sql-no-between-timestamp — Drizzle ORM backend.
//!
//! Flags Drizzle `between(col, a, b)` calls where the first argument is
//! a member expression whose property name carries a timestamp hint
//! (e.g. `users.createdAt`, `orders.updated_at`). `BETWEEN` is inclusive
//! on both sides, so it double-counts midnight rows on date ranges; the
//! correct shape is `gte(col, start)` + `lt(col, end)`.

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
        if name.contains('.') {
            return;
        }
        if name != "between" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let mut cursor = args.walk();
        let arg_nodes: Vec<tree_sitter::Node> = args
            .children(&mut cursor)
            .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
            .collect();
        if arg_nodes.is_empty() {
            return;
        }
        let first = arg_nodes[0];
        if first.kind() != "member_expression" {
            return;
        }
        let Some(prop) = first.child_by_field_name("property") else {
            return;
        };
        let Ok(prop_name) = prop.utf8_text(source_bytes) else {
            return;
        };
        if !looks_like_timestamp_column(prop_name) {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`between()` on a timestamp column has an off-by-one — \
                      use `gte(col, start)` and `lt(col, end)` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if `name` carries a hint that the column is a timestamp/date
/// (e.g. `createdAt`, `updated_at`, `expiryDate`, `eventTime`).
fn looks_like_timestamp_column(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with("_at")
        || lower.ends_with("at") && has_camel_at_suffix(name)
        || lower.contains("time")
        || lower.contains("date")
        || lower.contains("timestamp")
        || lower.contains("created")
        || lower.contains("updated")
        || lower.contains("deleted")
        || lower.contains("expired")
}

/// True if `name` is camelCase and ends with `At` (e.g. `createdAt`).
fn has_camel_at_suffix(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    let n = bytes.len();
    bytes[n - 2] == b'A' && bytes[n - 1] == b't'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_between_on_camel_created_at() {
        let src = "where(between(users.createdAt, start, end));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_between_on_snake_updated_at() {
        let src = "where(between(orders.updated_at, d1, d2));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_between_on_price() {
        let src = "where(between(products.price, 10, 100));";
        assert!(run(src).is_empty());
    }
}
