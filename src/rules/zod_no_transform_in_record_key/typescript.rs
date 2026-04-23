//! zod-no-transform-in-record-key backend.
//!
//! Matches `call_expression` whose callee is the member `z.record`. Inspects
//! the first argument — the key schema — and walks it looking for a
//! `member_expression` whose `property` is `transform`. If any such node is
//! found inside the key-schema subtree we flag the `z.record(...)` call,
//! because a transformed key defeats Zod's ability to round-trip the record.

use crate::diagnostic::{Diagnostic, Severity};

fn contains_transform(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "member_expression"
        && let Some(prop) = node.child_by_field_name("property")
        && prop.utf8_text(source).unwrap_or("") == "transform"
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_transform(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }
    let Some(obj) = func.child_by_field_name("object") else { return; };
    if obj.utf8_text(source).unwrap_or("") != "z" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "record" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let Some(first_arg) = args.named_children(&mut cursor).next() else { return; };

    if !contains_transform(first_arg, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.transform()` in a `z.record()` key schema mutates object keys after validation — drop the transform or move it to the value schema.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_transform_in_record_key() {
        assert_eq!(
            run("const r = z.record(z.string().transform(s => s.toLowerCase()), z.number());")
                .len(),
            1
        );
    }

    #[test]
    fn flags_transform_in_record_key_chained() {
        assert_eq!(
            run("const r = z.record(z.string().trim().transform(s => s), valueSchema);").len(),
            1
        );
    }

    #[test]
    fn allows_plain_key_schema() {
        assert!(run("const r = z.record(z.string(), z.number());").is_empty());
    }

    #[test]
    fn allows_transform_in_value_schema() {
        assert!(
            run("const r = z.record(z.string(), z.number().transform(n => n * 2));").is_empty()
        );
    }

    #[test]
    fn ignores_unrelated_record_call() {
        assert!(run("const r = other.record(z.string().transform(s => s), v);").is_empty());
    }
}
