//! tanstack-start-loader-stale-time backend — flag `ensureQueryData(...)`
//! call expressions whose options object is missing `staleTime`, or sets
//! it below `MIN_STALE_TIME_MS`. The loader otherwise refetches during
//! navigation transitions.

use crate::diagnostic::{Diagnostic, Severity};

/// Read the literal numeric value of a tree-sitter `number` node, or
/// `None` if the node is not a parseable integer literal.
fn number_literal_value(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<u64> {
    if node.kind() != "number" {
        return None;
    }
    let text = node.utf8_text(source).ok()?;
    text.parse::<u64>().ok()
}

/// Find the `staleTime: <value>` pair inside an `object` node. Returns
/// `Some((pair_node, value_node))` on hit, `None` on miss.
fn find_stale_time<'a>(
    obj: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<(tree_sitter::Node<'a>, tree_sitter::Node<'a>)> {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let key_text = key.utf8_text(source).ok()?;
        let normalized = key_text.trim_matches(|c: char| c == '"' || c == '\'');
        if normalized == "staleTime" {
            let value = child.child_by_field_name("value")?;
            return Some((child, value));
        }
    }
    None
}

crate::ast_check! { on ["call_expression"] prefilter = ["ensureQueryData"] => |node, source, ctx, diagnostics|
    let Some(function) = node.child_by_field_name("function") else { return };

    // Match either `ensureQueryData(...)` or `<receiver>.ensureQueryData(...)`.
    let is_ensure = match function.kind() {
        "identifier" => function.utf8_text(source).ok() == Some("ensureQueryData"),
        "member_expression" => function
            .child_by_field_name("property")
            .and_then(|p| p.utf8_text(source).ok()) == Some("ensureQueryData"),
        _ => false,
    };
    if !is_ensure { return; }

    let Some(arguments) = node.child_by_field_name("arguments") else { return };
    let mut cursor = arguments.walk();
    let first_arg = arguments.children(&mut cursor).find(|c| c.is_named());
    let Some(first_arg) = first_arg else { return };
    if first_arg.kind() != "object" { return; }

    let min_stale_time = ctx.config.threshold("tanstack-start-loader-stale-time", "min_stale_time_ms", ctx.lang) as u64;
    match find_stale_time(first_arg, source) {
        Some((_pair, value)) => {
            if let Some(n) = number_literal_value(value, source)
                && n < min_stale_time
            {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "`staleTime: {n}` is below {min_stale_time}ms — loader data will refetch during navigation."
                    ),
                    Severity::Warning,
                ));
            }
        }
        None => {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!(
                    "`ensureQueryData` call is missing `staleTime` — set it to at least {min_stale_time}ms to avoid refetches during navigation."
                ),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_stale_time_below_threshold() {
        let src =
            r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f, staleTime: 1000 })"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_missing_stale_time() {
        let src = r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f })"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_stale_time_at_threshold() {
        let src =
            r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f, staleTime: 5000 })"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_stale_time_above_threshold() {
        let src =
            r#"loader: () => ensureQueryData({ queryKey: ['x'], queryFn: f, staleTime: 30000 })"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_commented_line() {
        let src = "// ensureQueryData({ queryKey: ['x'], queryFn: f })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn handles_multiline_call() {
        let src = r#"
            ensureQueryData({
              queryKey: ['users'],
              queryFn: fetchUsers,
              staleTime: 2000,
            })
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
