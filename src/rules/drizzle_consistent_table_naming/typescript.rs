//! Flag calls to `pgTable` / `mysqlTable` / `sqliteTable` whose first
//! string argument is not lowercase snake_case plural.

use crate::diagnostic::{Diagnostic, Severity};

const TABLE_CTORS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable"];

fn is_snake_lower(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Heuristic plural check: ends with `s` (optionally `es`/`ies`) or with a
/// known uncountable/-en-style ending. Too strict a check causes FPs on
/// legitimate singular tables (e.g. `metadata`), so we accept any word
/// ending in `s` or any word containing `_` with last segment ending `s`.
fn looks_plural(s: &str) -> bool {
    let last = s.rsplit('_').next().unwrap_or(s);
    last.ends_with('s') || last.ends_with("data") || last.ends_with("info")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let name = func.utf8_text(source).unwrap_or("");
    if !TABLE_CTORS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let mut first_str: Option<tree_sitter::Node<'_>> = None;
    for c in args.children(&mut cursor) {
        if c.kind() == "string" {
            first_str = Some(c);
            break;
        }
    }
    let Some(s_node) = first_str else { return };
    let raw = s_node.utf8_text(source).unwrap_or("");
    let table_name = raw.trim_matches(['"', '\'']);
    if is_snake_lower(table_name) && looks_plural(table_name) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &s_node,
        super::META.id,
        format!(
            "Table name `{table_name}` should be lowercase snake_case plural (e.g. `user_profiles`)."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_camel_case_table_name() {
        let src = "const t = pgTable('orderItems', { id: serial('id') })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_singular_table_name() {
        let src = "const t = pgTable('user', { id: serial('id') })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_snake_plural() {
        let src = "const t = pgTable('order_items', { id: serial('id') })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_simple_plural() {
        let src = "const t = pgTable('users', { id: serial('id') })";
        assert!(run(src).is_empty());
    }
}
