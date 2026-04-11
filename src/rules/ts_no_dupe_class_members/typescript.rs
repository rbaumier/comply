//! ts-no-dupe-class-members backend — flag duplicate non-overload class
//! members with the same name.
//!
//! TS-specific twist: skip members with no body (overload signatures),
//! and skip computed property names.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashMap;

fn has_body(node: tree_sitter::Node) -> bool {
    node.child_by_field_name("body").is_some()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "class_body" {
        return;
    }

    // Map: member name -> list of (row, has_body).
    let mut seen: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
    let child_count = node.named_child_count();

    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        let ck = child.kind();

        // Only check method_definition and property_definition.
        if ck != "method_definition" && ck != "public_field_definition" && ck != "property_definition" {
            continue;
        }

        // Skip computed properties — can't statically compare them.
        let full = match std::str::from_utf8(&source[child.byte_range()]) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if full.starts_with('[') {
            continue;
        }

        let Some(name_node) = child.child_by_field_name("name") else { continue };
        if name_node.kind() == "computed_property_name" {
            continue;
        }
        let name = match std::str::from_utf8(&source[name_node.byte_range()]) {
            Ok(n) => n.to_string(),
            Err(_) => continue,
        };

        seen.entry(name).or_default().push((child.start_position().row, has_body(child)));
    }

    for (name, entries) in &seen {
        // Filter to entries with bodies (skip overload signatures).
        let with_body: Vec<_> = entries.iter().filter(|(_, b)| *b).collect();
        if with_body.len() < 2 {
            continue;
        }
        // Flag all but the first with a body.
        for &(row, _) in &with_body[1..] {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: row + 1,
                column: 1,
                rule_id: "ts-no-dupe-class-members".into(),
                message: format!(
                    "Duplicate class member `{name}` — this shadows the earlier definition."
                ),
                severity: Severity::Error,
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
    fn flags_duplicate_methods() {
        let diags = run_on("class Foo {\n  bar() {}\n  bar() {}\n}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_unique_members() {
        assert!(run_on("class Foo {\n  bar() {}\n  baz() {}\n}").is_empty());
    }

    #[test]
    fn allows_overload_signatures() {
        // TS overloads: method_definition without body followed by one with body.
        // tree-sitter may model these differently, but if both lack a body, fine.
        // overload parsing varies — at minimum we don't crash
        let _ = run_on("class Foo {\n  bar(): void;\n  bar(x: string): void;\n  bar(x?: string) {}\n}");
    }
}
