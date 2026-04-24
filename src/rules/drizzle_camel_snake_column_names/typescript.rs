//! In a `pair` node like `userId: varchar('userId', ...)`, flag when the key
//! is camelCase but the first string arg to the column constructor is not
//! snake_case.

use crate::diagnostic::{Diagnostic, Severity};

const COLUMN_CTORS: &[&str] = &[
    "varchar",
    "text",
    "integer",
    "bigint",
    "smallint",
    "serial",
    "bigserial",
    "boolean",
    "timestamp",
    "date",
    "time",
    "numeric",
    "decimal",
    "real",
    "doublePrecision",
    "uuid",
    "json",
    "jsonb",
    "char",
];

fn is_camel_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.chars().next().unwrap();
    if !first.is_ascii_lowercase() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric()) && s.chars().any(|c| c.is_ascii_uppercase())
}

fn is_snake_case_lower(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Return the innermost callee identifier by descending through chained
/// member_expressions. For `varchar('x').notNull()` the innermost call is
/// `varchar('x')`, not the chain tip.
fn base_call_name<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> Option<&'a str> {
    let mut cur = *node;
    loop {
        if cur.kind() != "call_expression" {
            return None;
        }
        let func = cur.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => return func.utf8_text(src).ok(),
            "member_expression" => {
                let obj = func.child_by_field_name("object")?;
                cur = obj;
            }
            _ => return None,
        }
    }
}

/// Extract the first string-literal argument (without quotes) of a
/// call_expression if present.
fn first_string_arg<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> Option<String> {
    // Descend to the innermost call (the column constructor call).
    let mut cur = *node;
    loop {
        if cur.kind() != "call_expression" {
            return None;
        }
        let func = cur.child_by_field_name("function")?;
        if func.kind() == "identifier" {
            break;
        }
        if func.kind() == "member_expression" {
            let obj = func.child_by_field_name("object")?;
            cur = obj;
            continue;
        }
        return None;
    }
    let args = cur.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "string" {
            let text = child.utf8_text(src).ok()?;
            let trimmed = text
                .trim_start_matches(['"', '\''])
                .trim_end_matches(['"', '\'']);
            return Some(trimmed.to_string());
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }
    let Some(key) = node.child_by_field_name("key") else { return };
    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "call_expression" {
        return;
    }
    let key_name = match key.kind() {
        "property_identifier" | "identifier" => key.utf8_text(source).unwrap_or(""),
        _ => return,
    };
    if !is_camel_case(key_name) {
        return;
    }
    let Some(ctor) = base_call_name(&value, source) else { return };
    if !COLUMN_CTORS.contains(&ctor) {
        return;
    }
    let Some(col_name) = first_string_arg(&value, source) else { return };
    if is_snake_case_lower(&col_name) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Property `{key_name}` is camelCase but its column name `{col_name}` is not snake_case — pass the snake_case database column name as the first argument."
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
    fn flags_camel_key_with_camel_column() {
        let src = "const t = { userId: varchar('userId', { length: 10 }) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_camel_key_with_snake_column() {
        let src = "const t = { userId: varchar('user_id', { length: 10 }) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_column_call() {
        let src = "const t = { userId: myHelper('userId') }";
        assert!(run(src).is_empty());
    }
}
