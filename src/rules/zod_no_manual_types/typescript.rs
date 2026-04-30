//! zod-no-manual-types backend — flag `type X = { ... }` aliases whose keys
//! exactly match a Zod object schema declared in the same file.
//!
//! Heuristic (kept local to avoid cross-file work):
//! 1. Walk the file once collecting every `const Name = z.object({ keys })`.
//! 2. For each `type_alias_declaration` whose right-hand side is an
//!    `object_type`, collect its property names.
//! 3. Flag the alias if some collected schema has the *exact* same key set
//!    (order-independent) and the alias is not using `z.infer`.

use std::collections::BTreeSet;
use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};

fn collect_object_keys<'a>(obj: Node<'a>, source: &'a [u8]) -> Option<BTreeSet<String>> {
    if obj.kind() != "object" {
        return None;
    }
    let mut keys = BTreeSet::new();
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(t) = k.utf8_text(source) else { continue };
        keys.insert(t.trim_matches(|c: char| c == '"' || c == '\'').to_string());
    }
    if keys.is_empty() { None } else { Some(keys) }
}

fn collect_schema_key_sets(root: Node<'_>, source: &[u8]) -> Vec<BTreeSet<String>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(f) = n.child_by_field_name("function")
            && f.utf8_text(source)
                .map(|t| t == "z.object")
                .unwrap_or(false)
            && let Some(args) = n.child_by_field_name("arguments")
        {
            let mut ac = args.walk();
            for a in args.named_children(&mut ac) {
                if let Some(keys) = collect_object_keys(a, source) {
                    out.push(keys);
                }
            }
        }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
    out
}

fn alias_keys<'a>(type_obj: Node<'a>, source: &'a [u8]) -> Option<BTreeSet<String>> {
    if type_obj.kind() != "object_type" {
        return None;
    }
    let mut keys = BTreeSet::new();
    let mut cursor = type_obj.walk();
    for child in type_obj.named_children(&mut cursor) {
        // property_signature has a name field in tree-sitter-typescript.
        if child.kind() == "property_signature"
            && let Some(name) = child.child_by_field_name("name")
            && let Ok(t) = name.utf8_text(source)
        {
            keys.insert(t.trim_matches(|c: char| c == '"' || c == '\'').to_string());
        }
    }
    if keys.is_empty() { None } else { Some(keys) }
}

crate::ast_check! { on ["program"] prefilter = ["z.infer"] => |node, source, ctx, diagnostics|
    // Only run once per file: fire on program / module root.
    let schemas = collect_schema_key_sets(node, source);
    if schemas.is_empty() { return; }

    // Walk to find type_alias_declaration nodes.
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "type_alias_declaration" {
            // Skip aliases that already use z.infer (check source contains it).
            let alias_text = n.utf8_text(source).unwrap_or("");
            if !alias_text.contains("z.infer")
                && let Some(value) = n.child_by_field_name("value")
                    && let Some(k) = alias_keys(value, source)
                        && schemas.iter().any(|s| s == &k) {
                            let pos = n.start_position();
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: super::META.id.into(),
                                message: "This `type` alias duplicates a Zod schema in the same file — \
                                          use `z.infer<typeof Schema>` instead so the type stays in sync.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
        }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_duplicated_type() {
        let src = "const UserSchema = z.object({ id: z.string(), name: z.string() });\n\
                   type User = { id: string; name: string };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_z_infer() {
        let src = "const UserSchema = z.object({ id: z.string(), name: z.string() });\n\
                   type User = z.infer<typeof UserSchema>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_type() {
        let src = "const UserSchema = z.object({ id: z.string() });\n\
                   type Other = { slug: string };";
        assert!(run(src).is_empty());
    }
}
