//! Flags `authenticateAsync()` calls whose enclosing function doesn't also
//! call `hasHardwareAsync` or `isEnrolledAsync`.

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

/// Earliest start-byte offset for a `call_expression` whose callee text
/// ends with `needle`, searched within `root`. Used to compare positions
/// between the hardware checks and `authenticateAsync`.
fn first_call_offset(root: tree_sitter::Node<'_>, source: &[u8], needle: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && let Ok(text) = func.utf8_text(source)
            && text.ends_with(needle)
        {
            let start = n.start_byte();
            best = Some(best.map_or(start, |b| b.min(start)));
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    best
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    let Ok(name) = func.utf8_text(source) else { return };
    if !name.ends_with("authenticateAsync") { return; }
    let Some(body) = enclosing_fn_body(node) else { return };

    let auth_offset = node.start_byte();
    let hw_before = first_call_offset(body, source, "hasHardwareAsync")
        .is_some_and(|o| o < auth_offset);
    let enrolled_before = first_call_offset(body, source, "isEnrolledAsync")
        .is_some_and(|o| o < auth_offset);
    if hw_before && enrolled_before { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`authenticateAsync` without `hasHardwareAsync` / `isEnrolledAsync` (in that order) — the call can fail on devices without biometrics.".into(),
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
    fn flags_without_checks() {
        let src = "async function unlock() { await LocalAuthentication.authenticateAsync(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_only_hardware() {
        let src = "async function unlock() { await LocalAuthentication.hasHardwareAsync(); await LocalAuthentication.authenticateAsync(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_both_checks() {
        let src = "async function unlock() { await LocalAuthentication.hasHardwareAsync(); await LocalAuthentication.isEnrolledAsync(); await LocalAuthentication.authenticateAsync(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_when_checks_come_after_authenticate() {
        // Both checks exist, but they run AFTER authenticateAsync — useless,
        // the call has already failed by then.
        let src = "async function unlock() { await LocalAuthentication.authenticateAsync(); await LocalAuthentication.hasHardwareAsync(); await LocalAuthentication.isEnrolledAsync(); }";
        assert_eq!(run(src).len(), 1);
    }
}
