//! AST backend for react-no-blocking-log-after-mutation.
//!
//! Only checks `export async function` declarations. Walks the
//! top-level statements and detects an `await <log/analytics/track>(...)`
//! that appears *after* another `await` expression.

use crate::diagnostic::{Diagnostic, Severity};

const LOG_NAMES: &[&str] = &["log", "logger", "analytics", "track", "telemetry", "metrics"];

fn is_exported_async_function(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "function_declaration" {
        return false;
    }
    // Async modifier.
    let text = node.utf8_text(source).unwrap_or("");
    if !text.trim_start().starts_with("async") && !text.contains("async function") {
        return false;
    }
    // Exported: parent is `export_statement`.
    let Some(parent) = node.parent() else { return false };
    parent.kind() == "export_statement"
}

/// Find the body of an exported async arrow function:
/// `export const action = async (...) => { ... }`.
///
/// Returns the arrow_function node when the pattern matches.
fn exported_async_arrow_body<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    // `node` is an `arrow_function`. Walk up: arrow_function ->
    // variable_declarator -> lexical_declaration -> export_statement.
    if node.kind() != "arrow_function" {
        return None;
    }
    // Must be async.
    let text = node.utf8_text(source).unwrap_or("");
    if !text.trim_start().starts_with("async") {
        return None;
    }
    let parent = node.parent()?;
    if parent.kind() != "variable_declarator" {
        return None;
    }
    let grandparent = parent.parent()?;
    if grandparent.kind() != "lexical_declaration" && grandparent.kind() != "variable_declaration" {
        return None;
    }
    let great = grandparent.parent()?;
    if great.kind() != "export_statement" {
        return None;
    }
    node.child_by_field_name("body")
}

fn await_call_target<'a>(
    await_node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    // await_expression -> expression (argument).
    let mut cursor = await_node.walk();
    let arg = await_node.named_children(&mut cursor).next()?;
    if arg.kind() != "call_expression" {
        return None;
    }
    let callee = arg.child_by_field_name("function")?;
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok(),
        "member_expression" => {
            // Match on the root object — `analytics.track(...)` → "analytics".
            let obj = callee.child_by_field_name("object")?;
            if obj.kind() == "identifier" {
                obj.utf8_text(source).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_log_target(name: &str) -> bool {
    LOG_NAMES.contains(&name)
}

fn collect_awaits<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    out: &mut Vec<(tree_sitter::Node<'a>, bool)>,
) {
    if node.kind() == "await_expression" {
        let is_log = await_call_target(node, source)
            .map(is_log_target)
            .unwrap_or(false);
        out.push((node, is_log));
        return;
    }
    // Do not descend into nested functions.
    match node.kind() {
        "function_declaration"
        | "function_expression"
        | "arrow_function"
        | "method_definition" => return,
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_awaits(child, source, out);
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    let body = if is_exported_async_function(node, source) {
        node.child_by_field_name("body")
    } else {
        exported_async_arrow_body(node, source)
    };
    let Some(body) = body else { return };
    let mut awaits = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        collect_awaits(child, source, &mut awaits);
    }
    let mut saw_non_log = false;
    for (await_node, is_log) in awaits {
        if !is_log {
            saw_non_log = true;
            continue;
        }
        if saw_non_log {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &await_node,
                super::META.id,
                "`await` on a log/analytics/track call after a main mutation blocks the response — \
                 drop the `await` or use `after()`/`waitUntil()`."
                    .into(),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_await_log_after_mutation() {
        let src = r#"
export async function createUser(data) {
  const user = await db.insert(data);
  await analytics.track("user_created", user);
  return user;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_log_before_mutation() {
        let src = r#"
export async function createUser(data) {
  await analytics.track("user_attempt");
  const user = await db.insert(data);
  return user;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_fire_and_forget_log() {
        let src = r#"
export async function createUser(data) {
  const user = await db.insert(data);
  analytics.track("user_created", user);
  return user;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_exported_function() {
        let src = r#"
async function createUser(data) {
  const user = await db.insert(data);
  await analytics.track("user_created");
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_exported_async_arrow() {
        // `export const action = async (...) => { ... }` is also a
        // server action shape — must be checked too.
        let src = r#"
export const action = async (data) => {
  const user = await db.insert(data);
  await analytics.track("user_created", user);
  return user;
};
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_exported_async_arrow_log_first() {
        let src = r#"
export const action = async (data) => {
  await analytics.track("attempt");
  const user = await db.insert(data);
  return user;
};
"#;
        assert!(run(src).is_empty());
    }
}
