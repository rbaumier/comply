//! ts-adjacent-overload-signatures backend — walk containers (program,
//! class_body, interface_body, object_type) and flag non-adjacent overload
//! signatures that share the same name.
//!
//! In tree-sitter-typescript, overloads appear as consecutive
//! `function_signature` / `method_signature` / `function_declaration` nodes
//! with the same name. When another declaration is interleaved, we flag it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    // Only operate on container nodes that can hold overloads.
    if kind != "program"
        && kind != "class_body"
        && kind != "interface_body"
        && kind != "object_type"
        && kind != "statement_block"
        && kind != "module" // TS namespace body
    {
        return;
    }

    // Collect named members in order.
    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();

    // Track: name -> bool (seen before), last_name
    let mut seen: Vec<String> = Vec::new();
    let mut last_name: Option<String> = None;

    for child in &children {
        let name = extract_overload_name(child, source);
        let Some(name) = name else {
            last_name = None;
            continue;
        };

        let is_adjacent = last_name.as_deref() == Some(&name);
        let was_seen = seen.contains(&name);

        if was_seen && !is_adjacent {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-adjacent-overload-signatures".into(),
                message: format!("All `{name}` signatures should be adjacent."),
                severity: Severity::Warning,
            });
        } else if !was_seen {
            seen.push(name.clone());
        }

        last_name = Some(name);
    }
}

/// Extract the function/method name from a node that could be an overload
/// signature or declaration. Returns `None` for non-relevant nodes.
fn extract_overload_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let kind = node.kind();
    match kind {
        // function foo(): void;  /  function foo() { ... }
        "function_signature" | "function_declaration" => {
            let name_node = node.child_by_field_name("name")?;
            let text = std::str::from_utf8(&source[name_node.byte_range()]).ok()?;
            Some(text.to_string())
        }
        // In class_body / interface_body: method_signature, method_definition
        "method_signature" | "method_definition" => {
            let name_node = node.child_by_field_name("name")?;
            let text = std::str::from_utf8(&source[name_node.byte_range()]).ok()?;
            // Prefix with "static " if static, to distinguish static vs instance.
            let is_static = (0..node.child_count()).any(|i| {
                node.child(i)
                    .map(|c| c.kind() == "static")
                    .unwrap_or(false)
            });
            if is_static {
                Some(format!("static {text}"))
            } else {
                Some(text.to_string())
            }
        }
        // call_signature / construct_signature in interfaces
        "call_signature" => Some("call".to_string()),
        "construct_signature" => Some("new".to_string()),
        // export function ... — unwrap the export
        "export_statement" => {
            let decl = node.child_by_field_name("declaration")?;
            extract_overload_name(&decl, source)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_non_adjacent_overloads() {
        let diags = run_on(
            r#"
function foo(): void;
function bar(): void;
function foo(x: number): void;
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_adjacent_overloads() {
        let diags = run_on(
            r#"
function foo(): void;
function foo(x: number): void;
function bar(): void;
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_in_interface() {
        let diags = run_on(
            r#"
interface I {
    foo(): void;
    bar(): void;
    foo(x: number): void;
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }
}
