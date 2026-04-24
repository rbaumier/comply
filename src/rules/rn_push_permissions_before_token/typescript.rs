//! Flags `getExpoPushTokenAsync()` calls whose enclosing function does not
//! also call `requestPermissionsAsync(...)`.

use crate::diagnostic::{Diagnostic, Severity};

fn enclosing_fn_body<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut current = node.parent();
    while let Some(n) = current {
        match n.kind() {
            "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "function" => {
                return n.child_by_field_name("body");
            }
            _ => {}
        }
        current = n.parent();
    }
    None
}

fn subtree_calls(root: tree_sitter::Node<'_>, source: &[u8], needle: &str) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && let Ok(text) = func.utf8_text(source)
            && text.ends_with(needle)
        {
            return true;
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn callee_name<'a>(call: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let func = call.child_by_field_name("function")?;
    let text = func.utf8_text(source).ok()?;
    Some(text)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(name) = callee_name(node, source) else { return };
    if !name.ends_with("getExpoPushTokenAsync") { return; }
    let Some(body) = enclosing_fn_body(node) else { return };
    if subtree_calls(body, source, "requestPermissionsAsync") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`getExpoPushTokenAsync` without a preceding `requestPermissionsAsync` — request notification permissions first.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_missing_permissions() {
        let src = "async function reg() { const t = await Notifications.getExpoPushTokenAsync(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_permissions() {
        let src = "async function reg() { await Notifications.requestPermissionsAsync(); const t = await Notifications.getExpoPushTokenAsync(); }";
        assert!(run(src).is_empty());
    }
}
