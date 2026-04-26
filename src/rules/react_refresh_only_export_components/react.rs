//! react-refresh-only-export-components AST backend.
//!
//! Flags non-component exports alongside component exports in `.tsx`/`.jsx`
//! files — this breaks React Fast Refresh (HMR).

use crate::diagnostic::{Diagnostic, Severity};

fn is_pascal_case(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Extract the exported name from an export_statement AST node.
/// Returns None for type/interface exports, re-exports, and wildcards.
fn extract_export_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let Ok(text) = node.utf8_text(source) else { return None };

    // Skip re-exports and wildcards.
    if text.contains(" from ") || text.starts_with("export *") {
        return None;
    }

    // Skip type/interface exports.
    if text.contains("export type ") || text.contains("export interface ") {
        return None;
    }

    // Walk named children to find the declaration.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration" | "class_declaration" => {
                if let Some(name) = child.child_by_field_name("name")
                    && let Ok(t) = name.utf8_text(source)
                {
                    return Some(t.to_string());
                }
            }
            "lexical_declaration" => {
                // `export const Foo = ...`
                let mut inner_cursor = child.walk();
                for decl in child.named_children(&mut inner_cursor) {
                    if decl.kind() == "variable_declarator"
                        && let Some(name) = decl.child_by_field_name("name")
                        && let Ok(t) = name.utf8_text(source)
                    {
                        return Some(t.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Only check the program (top-level) node once.
    // Only fire on .tsx/.jsx files.
    let path_str = ctx.path.to_string_lossy();
    if !path_str.ends_with(".tsx") && !path_str.ends_with(".jsx") {
        return;
    }

    let mut component_exports: Vec<String> = Vec::new();
    let mut non_component_exports: Vec<(String, usize)> = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }

        if let Some(name) = extract_export_name(child, source) {
            let line = child.start_position().row + 1;
            if is_pascal_case(&name) {
                component_exports.push(name);
            } else {
                non_component_exports.push((name, line));
            }
        }
    }

    if component_exports.is_empty() || non_component_exports.is_empty() {
        return;
    }

    for (name, line) in &non_component_exports {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: *line,
            column: 1,
            rule_id: "react-refresh-only-export-components".into(),
            message: format!(
                "Non-component export `{name}` alongside component exports breaks React Fast Refresh. Move it to a separate module."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_mixed_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export const helper = () => {};
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn allows_component_only_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export function AnotherComponent() { return <span />; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_type_exports_with_components() {
        let source = r#"
export type Props = { name: string };
export interface Config { debug: boolean }
export function MyComponent() { return <div />; }
"#;
        assert!(run(source).is_empty());
    }
}
