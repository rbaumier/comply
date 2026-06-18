//! data-clumps Rust backend — flag structs sharing 3+ identical field names.
//!
//! Walks the AST to find `struct_item` nodes, extracts their field names,
//! and flags when the same 3-field subset appears in 2+ structs.
//!
//! Borrowed "view" structs (a lifetime parameter plus at least one
//! reference-typed field) are excluded: they intentionally mirror an owned
//! struct's field names but cannot be merged with it.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::{FxHashMap, FxHashSet};

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir {
        return;
    }

    let mut struct_fields: Vec<(usize, Vec<String>)> = Vec::new();
    collect_structs(node, source, &mut struct_fields);

    // For each 3-field subset, count how many structs contain it.
    let mut subset_occurrences: FxHashMap<Vec<String>, Vec<usize>> = FxHashMap::default();
    for (line, fields) in &struct_fields {
        for combo in combinations(fields, 3) {
            subset_occurrences.entry(combo).or_default().push(*line);
        }
    }

    let mut flagged_lines: FxHashSet<usize> = FxHashSet::default();
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
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: "data-clumps".into(),
            message,
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Recursively collect struct field sets from the AST.
fn collect_structs(node: tree_sitter::Node, source: &[u8], out: &mut Vec<(usize, Vec<String>)>) {
    if node.kind() == "struct_item" {
        if crate::rules::rust_helpers::is_in_test_context(node, source) {
            return;
        }
        // Look for field_declaration_list child.
        let mut names: Vec<String> = Vec::new();
        let child_count = node.named_child_count();
        for i in 0..child_count {
            if let Some(child) = node.named_child(i)
                && child.kind() == "field_declaration_list"
            {
                let field_count = child.named_child_count();
                for j in 0..field_count {
                    if let Some(field) = child.named_child(j)
                        && field.kind() == "field_declaration"
                        && let Some(name_node) = field.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(source)
                    {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        if names.len() >= 3 && !is_borrowed_view_struct(node) {
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

/// True if `struct_node` is a borrowed "view" type: it has a lifetime
/// parameter and at least one reference-typed field (e.g. `RealmRef<'a>`
/// with `&'a str` fields, mirroring an owned `Realm`). Such a struct
/// intentionally shares its field names with the owned version but cannot
/// be merged with it, so it does not participate in data-clump detection.
fn is_borrowed_view_struct(struct_node: tree_sitter::Node) -> bool {
    has_lifetime_param(struct_node) && has_reference_field(struct_node)
}

fn has_lifetime_param(struct_node: tree_sitter::Node) -> bool {
    let Some(tp) = struct_node.child_by_field_name("type_parameters") else {
        return false;
    };
    let mut cursor = tp.walk();
    tp.named_children(&mut cursor)
        .any(|c| c.kind() == "lifetime_parameter")
}

fn has_reference_field(struct_node: tree_sitter::Node) -> bool {
    let child_count = struct_node.named_child_count();
    for i in 0..child_count {
        if let Some(list) = struct_node.named_child(i)
            && list.kind() == "field_declaration_list"
        {
            let field_count = list.named_child_count();
            for j in 0..field_count {
                if let Some(field) = list.named_child(j)
                    && field.kind() == "field_declaration"
                    && let Some(ty) = field.child_by_field_name("type")
                    && type_contains_reference(ty)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn type_contains_reference(node: tree_sitter::Node) -> bool {
    if node.kind() == "reference_type" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(type_contains_reference)
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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

    #[test]
    fn no_fp_on_cfg_test_structs() {
        let src = r#"
struct Env {
    id: String,
    netns: Option<String>,
    new_pid_ns: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ArgVals<'a> {
        id: &'a str,
        netns: Option<&'a str>,
        new_pid_ns: bool,
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_owned_borrowed_pair_issue_1026() {
        let src = r#"
type SmallString = String;

pub struct Realm {
    scheme: SmallString,
    host: Option<SmallString>,
    port: Option<u16>,
}

pub struct RealmRef<'a> {
    scheme: &'a str,
    host: Option<&'a str>,
    port: Option<u16>,
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_lifetime_struct_without_reference_fields() {
        let src = r#"
use std::borrow::Cow;

struct Owned {
    x: String,
    y: String,
    z: String,
}

struct Lazy<'a> {
    x: Cow<'a, str>,
    y: Cow<'a, str>,
    z: String,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn still_flags_production_clumps() {
        let src = r#"
struct Env {
    id: String,
    netns: Option<String>,
    new_pid_ns: bool,
}

struct ArgVals {
    id: String,
    netns: Option<String>,
    new_pid_ns: bool,
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }
}
