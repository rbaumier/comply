//! ts-class-literal-property-style backend — default "fields" mode:
//! flag getter methods in class bodies that do nothing but return a literal.
//! These should be `readonly` fields instead.
//!
//! Tree-sitter structure:
//!   class_body > method_definition[kind=get] > statement_block > return_statement > literal

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["method_definition"] => |node, source, ctx, diagnostics|
    // Check it's a getter: `get name() { return <literal>; }`
    // In tree-sitter, the first child is the "get" keyword for getters.
    let mut is_getter = false;
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i)
            && child.kind() == "get" {
                is_getter = true;
                break;
            }
    }
    if !is_getter {
        return;
    }

    // Find the body (statement_block).
    let Some(body_node) = node.child_by_field_name("body") else {
        return;
    };

    // Must have exactly one named child: a return_statement.
    let mut body_cursor = body_node.walk();
    let named_children: Vec<_> = body_node.named_children(&mut body_cursor).collect();
    if named_children.len() != 1 {
        return;
    }
    let stmt = named_children[0];
    if stmt.kind() != "return_statement" {
        return;
    }

    // The return value must be a literal (string, number, true, false, null, template_string).
    let mut stmt_cursor = stmt.walk();
    let ret_children: Vec<_> = stmt.named_children(&mut stmt_cursor).collect();
    if ret_children.len() != 1 {
        return;
    }
    let ret_val = ret_children[0];
    let is_literal = matches!(
        ret_val.kind(),
        "string" | "number" | "true" | "false" | "null" | "template_string"
    );
    if !is_literal {
        return;
    }

    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let name = match std::str::from_utf8(&source[name_node.byte_range()]) {
        Ok(n) => n,
        Err(_) => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-class-literal-property-style".into(),
        message: format!("Getter `{name}` returns a literal — use a `readonly` field instead."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_getter_returning_string_literal() {
        let diags = run_on(
            r#"
class Foo {
    get name() { return "hello"; }
}
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("readonly"));
    }

    #[test]
    fn flags_getter_returning_number_literal() {
        let diags = run_on(
            r#"
class Foo {
    get count() { return 42; }
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_getter_returning_expression() {
        let diags = run_on(
            r#"
class Foo {
    get name() { return this._name; }
}
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_readonly_field() {
        let diags = run_on(
            r#"
class Foo {
    readonly name = "hello";
}
"#,
        );
        assert!(diags.is_empty());
    }
}
