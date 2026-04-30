//! ts-unified-signatures backend — flag adjacent function overload signatures
//! in interfaces/type literals that share the same name.
//!
//! Simplified heuristic: if two or more `method_signature` or
//! `call_signature` nodes with the same name appear in the same
//! interface/type body, flag the duplicates.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashMap;

crate::ast_check! { on ["interface_body", "object_type"] => |node, source, ctx, diagnostics|
    // Collect method signatures grouped by name.
    let mut seen: HashMap<String, Vec<usize>> = HashMap::new();
    let child_count = node.named_child_count();

    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        let child_kind = child.kind();

        // call_signature has no name — group them all under a sentinel.
        if child_kind == "call_signature" {
            seen.entry("[[call]]".to_string()).or_default().push(child.start_position().row);
            continue;
        }

        if child_kind != "method_signature" {
            continue;
        }

        let Some(name_node) = child.child_by_field_name("name") else { continue };
        let name = match std::str::from_utf8(&source[name_node.byte_range()]) {
            Ok(n) => n.to_string(),
            Err(_) => continue,
        };
        seen.entry(name).or_default().push(child.start_position().row);
    }

    for (name, rows) in &seen {
        if rows.len() < 2 {
            continue;
        }
        // Flag all but the first occurrence.
        for &row in &rows[1..] {
            let display_name = if name == "[[call]]" {
                "Call signatures".to_string()
            } else {
                format!("`{name}` signatures")
            };
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: row + 1,
                column: 1,
                rule_id: "ts-unified-signatures".into(),
                message: format!(
                    "{display_name} can be unified into a single signature \
                     with a union or optional parameter."
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_method_signatures() {
        let diags = run_on("interface Foo {\n  bar(x: string): void;\n  bar(x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_different_method_names() {
        assert!(
            run_on("interface Foo {\n  bar(x: string): void;\n  baz(x: number): void;\n}")
                .is_empty()
        );
    }

    #[test]
    fn flags_duplicate_call_signatures() {
        let diags = run_on("interface Foo {\n  (x: string): void;\n  (x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }
}
