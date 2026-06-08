//! In a `pair` where the key is `createdAt`/`created_at` and the value is a
//! `timestamp(...)` column chain, flag if the chain does not contain
//! `.defaultNow(`.

use crate::diagnostic::{Diagnostic, Severity};

fn base_call_name<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> Option<&'a str> {
    let mut cur = *node;
    loop {
        if cur.kind() != "call_expression" {
            return None;
        }
        let func = cur.child_by_field_name("function")?;
        match func.kind() {
            "identifier" => return func.utf8_text(src).ok(),
            "member_expression" => {
                let obj = func.child_by_field_name("object")?;
                cur = obj;
            }
            _ => return None,
        }
    }
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return };
    let key_name = match key.kind() {
        "property_identifier" | "identifier" | "string" => {
            let t = key.utf8_text(source).unwrap_or("");
            t.trim_matches(['"', '\'']).to_string()
        }
        _ => return,
    };
    if key_name != "createdAt" && key_name != "created_at" {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "call_expression" {
        return;
    }
    let Some(ctor) = base_call_name(&value, source) else { return };
    if ctor != "timestamp" {
        return;
    }
    let chain_text = value.utf8_text(source).unwrap_or("");
    if chain_text.contains(".defaultNow(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`createdAt` timestamp column must chain `.defaultNow()` so inserts auto-populate the value.".into(),
        Severity::Warning,
    ));
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_created_at_without_default_now() {
        let src = "const t = { createdAt: timestamp('created_at').notNull() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_created_at_with_default_now() {
        let src = "const t = { createdAt: timestamp('created_at').defaultNow().notNull() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_timestamp() {
        let src = "const t = { createdAt: text('created_at') }";
        assert!(run(src).is_empty());
    }
}
