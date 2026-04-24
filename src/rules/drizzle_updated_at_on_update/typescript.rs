//! Flag `updatedAt` / `updated_at` column definitions that don't chain
//! `.$onUpdate(`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("");
    let key_name = key_text.trim_matches(['"', '\'']);
    if key_name != "updatedAt" && key_name != "updated_at" {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "call_expression" {
        return;
    }
    let chain_text = value.utf8_text(source).unwrap_or("");
    if chain_text.contains(".$onUpdate(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`updatedAt` must chain `.$onUpdate(() => new Date())` so the column is refreshed on every update.".into(),
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
    fn flags_updated_at_without_on_update() {
        let src = "const t = { updatedAt: timestamp('updated_at').defaultNow() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_updated_at_with_on_update() {
        let src = "const t = { updatedAt: timestamp('updated_at').defaultNow().$onUpdate(() => new Date()) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_keys() {
        let src = "const t = { createdAt: timestamp('created_at') }";
        assert!(run(src).is_empty());
    }
}
