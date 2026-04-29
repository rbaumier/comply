//! zod-prefer-discriminated-union backend — flag `z.union([...])` calls
//! whose array argument contains `z.object({...})` branches sharing a
//! literal-tagged field (`type: z.literal(...)`, `kind: ...`,
//! `__type: ...`). Such unions parse faster and produce better errors as
//! `z.discriminatedUnion('type', [...])`.

use crate::diagnostic::{Diagnostic, Severity};

const TAG_KEYS: &[&str] = &["type", "kind", "__type"];

/// Return true if `obj` (a tree-sitter `object` node) contains a `pair`
/// whose key matches any tag key and whose value is a call to
/// `z.literal(...)`.
fn object_has_tag_literal(obj: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(key_text) = key.utf8_text(source) else {
            continue;
        };
        let normalized = key_text.trim_matches(|c: char| c == '"' || c == '\'');
        if !TAG_KEYS.iter().any(|k| *k == normalized) {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if value.kind() != "call_expression" {
            continue;
        }
        let Some(function) = value.child_by_field_name("function") else {
            continue;
        };
        if function.utf8_text(source).ok() == Some("z.literal") {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["z.union"] => |node, source, ctx, diagnostics|
    let Some(function) = node.child_by_field_name("function") else { return };
    if function.utf8_text(source).ok() != Some("z.union") { return; }

    let Some(arguments) = node.child_by_field_name("arguments") else { return };
    // Find the first named argument: must be an array.
    let mut cursor = arguments.walk();
    let array = arguments.children(&mut cursor).find(|c| c.is_named());
    let Some(array) = array else { return };
    if array.kind() != "array" { return; }

    let mut has_literal_tag = false;
    let mut acursor = array.walk();
    for elem in array.children(&mut acursor) {
        if elem.kind() != "call_expression" { continue; }
        let Some(elem_fn) = elem.child_by_field_name("function") else { continue };
        if elem_fn.utf8_text(source).ok() != Some("z.object") { continue; }
        let Some(elem_args) = elem.child_by_field_name("arguments") else { continue };
        let mut ecursor = elem_args.walk();
        let obj = elem_args.children(&mut ecursor).find(|c| c.is_named());
        let Some(obj) = obj else { continue };
        if obj.kind() != "object" { continue; }
        if object_has_tag_literal(obj, source) {
            has_literal_tag = true;
            break;
        }
    }

    if !has_literal_tag { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Replace `z.union([z.object({type: z.literal(...)}), ...])` with `z.discriminatedUnion('type', [...])` for faster parsing.".into(),
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
    fn flags_union_with_literals() {
        let src = "z.union([\n  z.object({ type: z.literal('a') }),\n  z.object({ type: z.literal('b') }),\n])";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_discriminated_union() {
        assert!(
            run("z.discriminatedUnion('type', [z.object({ type: z.literal('a') })])").is_empty()
        );
    }
}
