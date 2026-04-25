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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    if !is_sql_raw_callee(node, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    if first.kind() == "string" {
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
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
}
