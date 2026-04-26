//! tanstack-query-prefer-key-factory backend.
//!
//! Flag `queryKey: ['name', dynamicArg]` — a string prefix followed by a
//! variable element. Inline mixed keys are easy to drift across call
//! sites; a key factory keeps the shape consistent.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "queryKey" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "array" { return; }

    let mut cursor = value.walk();
    let mut has_string = false;
    let mut has_variable = false;
    for child in value.named_children(&mut cursor) {
        match child.kind() {
            "string" | "template_string" => has_string = true,
            "number" | "true" | "false" | "null" | "undefined" => {}
            _ => has_variable = true,
        }
    }
    if !(has_string && has_variable) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Extract dynamic `queryKey` to a key factory: `const keys = { detail: (id) => ['res', id] as const }`.".into(),
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
    fn flags_inline_dynamic_key() {
        assert_eq!(
            run("useQuery({ queryKey: ['todos', userId], queryFn: f })").len(),
            1
        );
    }

    #[test]
    fn allows_static_key() {
        assert!(run("useQuery({ queryKey: ['todos'], queryFn: f })").is_empty());
    }

    #[test]
    fn allows_factory() {
        assert!(run("useQuery({ queryKey: todoKeys.detail(userId), queryFn: f })").is_empty());
    }
}
