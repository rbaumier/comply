//! data-clumps Rust backend — flag structs sharing 3+ identical field names.
//!
//! Walks the AST to find `struct_item` nodes, extracts their field names,
//! and flags when the same 3-field subset appears in 2+ structs.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::{HashMap, HashSet};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    let mut struct_fields: Vec<(usize, Vec<String>)> = Vec::new();
    collect_structs(node, source, &mut struct_fields);

    // For each 3-field subset, count how many structs contain it.
    let mut subset_occurrences: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for (line, fields) in &struct_fields {
        for combo in combinations(fields, 3) {
            subset_occurrences.entry(combo).or_default().push(*line);
        }
    }

    let mut flagged_lines: HashSet<usize> = HashSet::new();
    let mut results: Vec<(usize, String)> = Vec::new();

    for (subset, lines) in &subset_occurrences {
        if lines.len() >= 2 {
            for &line in lines {
                if flagged_lines.insert(line) {
                    results.push((
                        line,
                        format!(
                            "Fields [{}] appear together in {} structs \
                             \u{2014} extract into a shared type.",
                            subset.join(", "),
                            lines.len(),
                        ),
                    ));
                }
            }
        }
    }

    results.sort_by_key(|(line, _)| *line);
    for (line, message) in results {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line,
            column: 1,
            rule_id: "data-clumps".into(),
            message,
            severity: Severity::Warning,
        });
    }
}

/// Recursively collect struct field sets from the AST.
fn collect_structs(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut Vec<(usize, Vec<String>)>,
) {
    if node.kind() == "struct_item" {
        // Look for field_declaration_list child.
        let mut names: Vec<String> = Vec::new();
        let child_count = node.named_child_count();
        for i in 0..child_count {
            if let Some(child) = node.named_child(i)
                && child.kind() == "field_declaration_list" {
                    let field_count = child.named_child_count();
                    for j in 0..field_count {
                        if let Some(field) = child.named_child(j)
                            && field.kind() == "field_declaration"
                            && let Some(name_node) = field.child_by_field_name("name")
                            && let Ok(name) = name_node.utf8_text(source) {
                                names.push(name.to_string());
                        }
                    }
            }
        }
        names.sort();
        names.dedup();
        if names.len() >= 3 {
            out.push((node.start_position().row + 1, names));
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_structs(cursor.node(), source, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Generate all sorted subsets of size `k` from `items`.
fn combinations(items: &[String], k: usize) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut combo = vec![0usize; k];
    fn recurse(
        items: &[String],
        k: usize,
        start: usize,
        combo: &mut Vec<usize>,
        depth: usize,
        result: &mut Vec<Vec<String>>,
    ) {
        if depth == k {
            result.push(combo[..k].iter().map(|&i| items[i].clone()).collect());
            return;
        }
        if start + (k - depth) > items.len() {
            return;
        }
        for i in start..items.len() {
            combo[depth] = i;
            recurse(items, k, i + 1, combo, depth + 1, result);
        }
    }
    recurse(items, k, 0, &mut combo, 0, &mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_repeated_field_group() {
        let src = r#"
struct CreateUser {
    name: String,
    email: String,
    age: u32,
}
struct UpdateUser {
    name: String,
    email: String,
    age: u32,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_different_fields() {
        let src = r#"
struct User {
    name: String,
    email: String,
    age: u32,
}
struct Email {
    to: String,
    subject: String,
    body: String,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fewer_than_three_shared() {
        let src = r#"
struct Foo {
    a: String,
    b: String,
    c: u32,
}
struct Bar {
    a: String,
    b: String,
    d: u32,
}
"#;
        assert!(run_on(src).is_empty());
    }
}
