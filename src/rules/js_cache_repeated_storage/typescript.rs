//! Flags repeated `localStorage.getItem("same-key")` calls in a function body.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

const STORAGE_OBJECTS: &[&str] = &["localStorage", "sessionStorage"];

fn collect_getitem_calls<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
    calls: &mut Vec<(String, tree_sitter::Node<'a>)>,
) {
    if node.kind() == "call_expression" {
        if let Some(callee) = node.child_by_field_name("function") {
            if callee.kind() == "member_expression" {
                let obj = callee
                    .child_by_field_name("object")
                    .and_then(|o| o.utf8_text(source).ok())
                    .unwrap_or("");
                let prop = callee
                    .child_by_field_name("property")
                    .and_then(|p| p.utf8_text(source).ok())
                    .unwrap_or("");
                if STORAGE_OBJECTS.contains(&obj) && prop == "getItem" {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        if let Some(first_arg) = args.children(&mut cursor).find(|c| c.is_named())
                        {
                            if first_arg.kind() == "string" || first_arg.kind() == "template_string"
                            {
                                let raw = first_arg.utf8_text(source).ok().unwrap_or("");
                                let key = raw.trim_matches(|c| c == '\'' || c == '"' || c == '`');
                                calls.push((format!("{obj}.{key}"), node));
                            }
                        }
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "arrow_function" | "function_expression" | "function_declaration"
        ) {
            continue;
        }
        collect_getitem_calls(child, source, calls);
    }
}

crate::ast_check! { on ["function_declaration", "arrow_function", "function_expression", "method_definition"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };

    let mut calls = Vec::new();
    collect_getitem_calls(body, source, &mut calls);

    let mut seen = HashSet::new();
    for (key, call_node) in &calls {
        if !seen.insert(key.clone()) {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: call_node.start_position().row + 1,
                column: call_node.start_position().column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Repeated `getItem(\"{}\")` — read once into a variable.",
                    key.split('.').next_back().unwrap_or(key)
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_repeated_get_item() {
        assert_eq!(
            run(r#"
function load() {
    const a = localStorage.getItem("token");
    const b = localStorage.getItem("token");
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_session_storage() {
        assert_eq!(
            run(r#"
function load() {
    const a = sessionStorage.getItem("key");
    const b = sessionStorage.getItem("key");
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn allows_different_keys() {
        assert!(run(r#"
function load() {
    const a = localStorage.getItem("token");
    const b = localStorage.getItem("user");
}
"#)
        .is_empty());
    }

    #[test]
    fn allows_single_call() {
        assert!(run(r#"
function load() {
    const a = localStorage.getItem("token");
}
"#)
        .is_empty());
    }
}
