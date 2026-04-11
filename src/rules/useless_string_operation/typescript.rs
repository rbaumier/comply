//! useless-string-operation backend — detect standalone string method calls
//! whose return value is not used, via tree-sitter AST.

use crate::diagnostic::{Diagnostic, Severity};

const STRING_METHODS: &[&str] = &[
    "replace",
    "replaceAll",
    "trim",
    "trimStart",
    "trimEnd",
    "toUpperCase",
    "toLowerCase",
    "substring",
    "slice",
    "concat",
    "padStart",
    "padEnd",
    "normalize",
    "repeat",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    // A standalone call is: expression_statement > call_expression
    if node.kind() != "expression_statement" {
        return;
    }

    let Some(expr) = node.named_child(0) else { return };
    if expr.kind() != "call_expression" {
        return;
    }

    let Some(callee) = expr.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = match prop.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    if !STRING_METHODS.contains(&method) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "useless-string-operation".into(),
        message: "String method result is ignored \u{2014} strings are immutable, \
                  the return value must be used."
            .into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_standalone_trim() {
        assert_eq!(run_on("name.trim();").len(), 1);
    }

    #[test]
    fn flags_standalone_replace() {
        assert_eq!(run_on(r#"str.replace("a", "b");"#).len(), 1);
    }

    #[test]
    fn flags_standalone_to_upper() {
        assert_eq!(run_on("title.toUpperCase();").len(), 1);
    }

    #[test]
    fn allows_assigned_trim() {
        assert!(run_on("const cleaned = name.trim();").is_empty());
    }

    #[test]
    fn allows_returned_value() {
        assert!(run_on("return name.trim();").is_empty());
    }

    #[test]
    fn allows_as_argument() {
        assert!(run_on("console.log(name.trim());").is_empty());
    }
}
