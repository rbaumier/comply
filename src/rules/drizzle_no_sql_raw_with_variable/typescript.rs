//! drizzle-no-sql-raw-with-variable — flag `sql.raw(...)` calls whose
//! argument is anything other than a plain string literal.
//!
//! AST detection: walk `call_expression` nodes whose callee is the
//! `member_expression` `sql.raw`. If the first argument's kind isn't
//! `string`, the value comes from a variable / template literal /
//! function call, which is the SQL injection vector.

use crate::diagnostic::{Diagnostic, Severity};

fn is_sql_raw_callee(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    obj.utf8_text(source).unwrap_or("") == "sql" && prop.utf8_text(source).unwrap_or("") == "raw"
}

/// Returns true when every `${...}` substitution in the template string is
/// wrapped in SQL double-quote identifier syntax — `"${expr}"`. Such calls
/// are safe DDL-identifier patterns; bare `${expr}` interpolations remain
/// flagged.
fn template_literal_is_safe(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "template_substitution" {
            let start = child.start_byte();
            let end = child.end_byte();
            let char_before = start.checked_sub(1).and_then(|i| source.get(i)).copied();
            let char_after = source.get(end).copied();
            if char_before != Some(b'"') || char_after != Some(b'"') {
                return false;
            }
        }
    }
    true
}

crate::ast_check! { on ["call_expression"] prefilter = ["sql.raw"] => |node, source, ctx, diagnostics|
    if !is_sql_raw_callee(node, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    // String literal → safe.
    if first.kind() == "string" {
        return;
    }
    // Template literal → safe when no substitutions or all substitutions are
    // wrapped in SQL double-quote identifier syntax.
    if first.kind() == "template_string" && template_literal_is_safe(first, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`sql.raw()` with a non-literal argument is a SQL injection vector — use `sql` tagged templates with parameterized values instead.".into(),
        Severity::Error,
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
    fn flags_variable_argument() {
        assert_eq!(run("sql.raw(userInput)").len(), 1);
    }

    #[test]
    fn flags_template_literal() {
        assert_eq!(run("sql.raw(`SELECT * FROM ${tableName}`)").len(), 1);
    }

    #[test]
    fn flags_mixed_quoted_and_unquoted() {
        assert_eq!(run(r#"sql.raw(`"${id}" WHERE col = ${value}`)"#).len(), 1);
    }

    #[test]
    fn allows_string_literal_double_quote() {
        assert!(run("sql.raw(\"SELECT 1\")").is_empty());
    }

    #[test]
    fn allows_string_literal_single_quote() {
        assert!(run("sql.raw('NOW()')").is_empty());
    }

    #[test]
    fn allows_tagged_template() {
        assert!(run("sql`WHERE id = ${userId}`").is_empty());
    }

    #[test]
    fn allows_static_template_literal() {
        assert!(run("sql.raw(`SELECT 1`)").is_empty());
    }

    /// Regression for issue #344: sql.raw with a DDL identifier from pg_class
    /// must not be flagged when the identifier is properly double-quoted.
    #[test]
    fn allows_double_quoted_identifier_in_template() {
        assert!(run(r#"sql.raw(`DROP INDEX IF EXISTS "${row.name}"`)"#).is_empty());
    }

    #[test]
    fn allows_multiple_double_quoted_identifiers() {
        assert!(run(r#"sql.raw(`ALTER TABLE "${schema}"."${table}" ADD COLUMN id int`)"#).is_empty());
    }
}
