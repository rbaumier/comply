//! mysql-no-multiple-statements — flag `mysql.createConnection({ multipleStatements: true })`
//! (and `createPool`) because enabling multi-statement queries amplifies SQL injection risk.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["mysql"] => |node, source, ctx, diagnostics|
    // Callee must be `mysql.createConnection` or `mysql.createPool` (member_expression).
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(object) = callee.child_by_field_name("object") else { return };
    let Some(property) = callee.child_by_field_name("property") else { return };

    let object_text = object.utf8_text(source).unwrap_or("");
    if object_text != "mysql" {
        return;
    }

    let property_text = property.utf8_text(source).unwrap_or("");
    if property_text != "createConnection" && property_text != "createPool" {
        return;
    }

    // Find the argument list and inspect the first object argument.
    let Some(arguments) = node.child_by_field_name("arguments") else { return };
    let mut cursor = arguments.walk();
    for arg in arguments.named_children(&mut cursor) {
        if arg.kind() != "object" {
            continue;
        }
        let mut obj_cursor = arg.walk();
        for pair in arg.named_children(&mut obj_cursor) {
            if pair.kind() != "pair" {
                continue;
            }
            let Some(key) = pair.child_by_field_name("key") else { continue };
            let key_text = key
                .utf8_text(source)
                .unwrap_or("")
                .trim_matches(|c: char| c == '\'' || c == '"');
            if key_text != "multipleStatements" {
                continue;
            }
            let Some(value) = pair.child_by_field_name("value") else { continue };
            if value.utf8_text(source).unwrap_or("").trim() != "true" {
                continue;
            }

            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &pair,
                super::META.id,
                "`multipleStatements: true` amplifies SQL injection risk — remove this option."
                    .into(),
                Severity::Error,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_create_connection_multiple_statements_true() {
        assert_eq!(
            run("mysql.createConnection({ host: 'localhost', multipleStatements: true })").len(),
            1
        );
    }

    #[test]
    fn flags_create_pool_multiple_statements_true() {
        assert_eq!(
            run("mysql.createPool({ multipleStatements: true, host: 'localhost' })").len(),
            1
        );
    }

    #[test]
    fn allows_multiple_statements_false() {
        assert!(run("mysql.createConnection({ multipleStatements: false })").is_empty());
    }

    #[test]
    fn allows_missing_option() {
        assert!(run("mysql.createConnection({ host: 'localhost' })").is_empty());
    }

    #[test]
    fn ignores_other_callers() {
        assert!(run("db.createConnection({ multipleStatements: true })").is_empty());
    }
}
