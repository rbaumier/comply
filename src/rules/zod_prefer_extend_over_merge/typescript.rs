//! zod-prefer-extend-over-merge backend — flag `.merge()` on schema-looking receivers.
//!
//! To reduce false positives on unrelated `.merge()` methods (e.g. lodash), we
//! only flag calls whose receiver chain contains a `z.` identifier.

use crate::diagnostic::{Diagnostic, Severity};

fn receiver_has_zod_root(mut node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    loop {
        match node.kind() {
            "identifier" => {
                return node.utf8_text(source).map(|t| t == "z").unwrap_or(false);
            }
            "member_expression" => {
                let Some(obj) = node.child_by_field_name("object") else { return false };
                node = obj;
            }
            "call_expression" => {
                let Some(f) = node.child_by_field_name("function") else { return false };
                node = f;
            }
            _ => return false,
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if prop_text != "merge" { return; }

    let Some(receiver) = func.child_by_field_name("object") else { return };
    // Must be on a zod-looking receiver (schema variable or chain). We approximate
    // by walking down the receiver looking for a `z` root, OR by matching the
    // receiver text against a zod-schema-looking pattern.
    let hit = receiver_has_zod_root(receiver, source) || {
        let Ok(t) = receiver.utf8_text(source) else { return };
        // Heuristic: receiver is an identifier ending in `Schema` (common convention).
        receiver.kind() == "identifier" && t.ends_with("Schema")
    };
    if !hit { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`.merge()` is removed in Zod v4 — use `.extend(other.shape)` \
                  to combine object schemas.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_merge_on_z_object() {
        assert_eq!(
            run("const S = z.object({ a: z.string() }).merge(Other);").len(),
            1
        );
    }

    #[test]
    fn flags_merge_on_schema_variable() {
        assert_eq!(run("const S = UserSchema.merge(AdminSchema);").len(), 1);
    }

    #[test]
    fn allows_extend() {
        assert!(run("const S = UserSchema.extend({ role: z.string() });").is_empty());
    }

    #[test]
    fn ignores_unrelated_merge() {
        assert!(run("const r = _.merge(a, b);").is_empty());
    }
}
