//! Flag template_string nodes containing `IN (` when used as a tagged
//! template with the `sql` tag.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the uppercased text contains `IN (` followed (after optional whitespace) by `SELECT`.
fn in_followed_by_select(upper: &str) -> bool {
    for prefix in [" IN (", "\nIN (", "\tIN ("] {
        let mut search = upper;
        while let Some(pos) = search.find(prefix) {
            let after = search[pos + prefix.len()..].trim_start_matches([' ', '\t', '\n', '\r']);
            if after.starts_with("SELECT") {
                return true;
            }
            search = &search[pos + 1..];
        }
    }
    false
}

crate::ast_check! { on ["template_string"] => |node, source, ctx, diagnostics|
    // tree-sitter-typescript exposes tagged templates either as
    // `template_string` children of `template_literal_type` or as
    // `call_expression`-like `template_substitution`. A simpler signal:
    // look for `template_string` whose parent is either a
    // `call_expression` with function `sql`, or just a standalone
    // `sql`-tagged template node.
    let Some(parent) = node.parent() else { return };
    // Tagged template: tree-sitter encodes `sql\`...\`` as
    // `call_expression` where function = "sql" and arguments = the template.
    // Some grammars produce a `template_literal` wrapper. We handle both.
    let tag_text = match parent.kind() {
        "call_expression" => {
            let f = parent.child_by_field_name("function");
            f.and_then(|f| f.utf8_text(source).ok()).unwrap_or("")
        }
        _ => {
            // Search previous sibling for an identifier tag.
            let prev = node.prev_sibling();
            prev.and_then(|p| p.utf8_text(source).ok()).unwrap_or("")
        }
    };
    if tag_text != "sql" {
        return;
    }
    let text = node.utf8_text(source).unwrap_or("");
    // Match `IN (` (case-insensitive).
    let upper = text.to_ascii_uppercase();
    if !upper.contains(" IN (") && !upper.contains("\nIN (") && !upper.contains("\tIN (") {
        return;
    }
    // PL/pgSQL DO blocks use dollar-quoting (`DO $$` or `DO $tag$`).
    // inArray() cannot be used inside them, so skip.
    if upper.contains("DO $") {
        return;
    }
    // `IN (SELECT ...)` is a subquery — inArray() does not support subqueries, skip.
    if in_followed_by_select(&upper) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`sql` template contains `IN (...)` — prefer `inArray(col, [...])` for a parameterized, typed alternative.".into(),
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
    fn flags_sql_template_with_in() {
        let src = "const q = sql`SELECT * FROM u WHERE id IN (${ids})`";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inarray_call() {
        let src = "const q = db.select().from(u).where(inArray(u.id, ids))";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_sql_template_without_in() {
        let src = "const q = sql`SELECT * FROM u WHERE id = ${id}`";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plpgsql_do_block_with_in() {
        // PL/pgSQL DO blocks cannot use inArray() — false positive from #345.
        let src = r#"database.execute(sql`
DO $$
DECLARE idx_name TEXT;
BEGIN
  FOR idx_name IN
    SELECT cls.relname FROM pg_class cls
    WHERE cls.relname NOT IN ('idx_foo', 'idx_bar')
  LOOP
    EXECUTE format('DROP INDEX IF EXISTS %I', idx_name);
  END LOOP;
END;
$$`)"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_subquery() {
        // inArray() does not support subqueries — false positive from #529.
        let src = r#"db.delete(account).where(sql`account.user_id IN (SELECT id FROM user WHERE email = ${email})`)"#;
        assert!(run(src).is_empty());
    }
}
