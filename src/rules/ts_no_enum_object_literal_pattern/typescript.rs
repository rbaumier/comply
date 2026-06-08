//! Detect:
//!
//! ```ignore
//! const Color = { red: 'r', blue: 'b' } as const;
//! const value = Color[someStringVar];   // ← flagged
//! ```
//!
//! Heuristic:
//! 1. Walk the file and collect identifiers introduced by
//!    `const NAME = { ... } as const`.
//! 2. For every `subscript_expression` (a.k.a. element access), if the
//!    object is one of those names AND the index is a non-literal expression
//!    (not a string/number, not a `keyof` cast), flag it.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

/// Walk the program and collect the names of `const X = { ... } as const`
/// bindings. We only return identifier names — that's enough for our match.
fn collect_as_const_objects(root: tree_sitter::Node, source: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
            // Iterate variable declarators.
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                if child.kind() != "variable_declarator" {
                    continue;
                }
                let Some(name_node) = child.child_by_field_name("name") else {
                    continue;
                };
                let Some(value_node) = child.child_by_field_name("value") else {
                    continue;
                };
                if value_node.kind() != "as_expression" {
                    continue;
                }
                // The text of an as_expression includes "<expr> as const".
                let text = value_node.utf8_text(source).unwrap_or("");
                if !text.trim_end().ends_with(" as const") && !text.trim_end().ends_with("as const")
                {
                    continue;
                }
                // The expression part should be an object literal.
                let mut found_object = false;
                let mut cc = value_node.walk();
                for sub in value_node.named_children(&mut cc) {
                    if sub.kind() == "object" {
                        found_object = true;
                        break;
                    }
                }
                if !found_object {
                    continue;
                }
                if name_node.kind() == "identifier" {
                    if let Ok(text) = name_node.utf8_text(source) {
                        names.insert(text.to_string());
                    }
                }
            }
        }
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            stack.push(child);
        }
    }
    names
}

/// Is the index node a literal we consider safe (string, number, keyof cast)?
fn is_safe_index(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "string" | "number" | "true" | "false" | "null" | "undefined" => true,
        "as_expression" | "type_assertion" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.contains("keyof ")
        }
        _ => false,
    }
}

crate::ast_check! {
    on ["subscript_expression"]
    => |node, source, ctx, diagnostics|
    let Some(object) = node.child_by_field_name("object") else { return; };
    let Some(index) = node.child_by_field_name("index") else { return; };
    if object.kind() != "identifier" { return; }
    let Ok(obj_name) = object.utf8_text(source) else { return; };
    if is_safe_index(index, source) { return; }

    // Find program root.
    let mut root = node;
    while let Some(parent) = root.parent() { root = parent; }
    let names = collect_as_const_objects(root, source);
    if !names.contains(obj_name) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Indexing `{obj_name}` (declared `as const`) with an arbitrary key widens the result \
             to a unioned type and skips the narrow lookup. Cast: `{obj_name}[k as keyof typeof {obj_name}]`."
        ),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_arbitrary_string_index() {
        let src = "const Color = { red: 'r', blue: 'b' } as const;\nfunction f(k: string) { return Color[k]; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_string_literal_index() {
        let src = "const Color = { red: 'r', blue: 'b' } as const;\nconst v = Color['red'];";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyof_cast_index() {
        let src = "const Color = { red: 'r' } as const;\nfunction f(k: string) { return Color[k as keyof typeof Color]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_as_const_object() {
        let src =
            "const Color = { red: 'r', blue: 'b' };\nfunction f(k: string) { return Color[k]; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_indexing() {
        let src = "function f(arr: string[], i: number) { return arr[i]; }";
        assert!(run(src).is_empty());
    }
}
