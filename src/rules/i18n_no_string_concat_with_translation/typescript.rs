//! i18n-no-string-concat-with-translation — flag binary `+`
//! expressions where one operand is a `t('...')` / `t("...")` call.
//!
//! AST detection: walk `binary_expression` nodes whose operator is `+`
//! and check whether either operand (recursively, to handle nested
//! concatenations) is a `call_expression` whose callee is `t` and
//! whose first argument is a string literal.

use crate::diagnostic::{Diagnostic, Severity};

fn is_t_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.utf8_text(source).unwrap_or("") != "t" {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .next()
        .is_some_and(|n| n.kind() == "string")
}

fn contains_t_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if is_t_call(node, source) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_t_call(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    if op_node.utf8_text(source).unwrap_or("") != "+" {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };
    if !contains_t_call(left, source) && !contains_t_call(right, source) {
        return;
    }
    // Skip nested binary_expression matches: only flag the outermost one.
    if node
        .parent()
        .is_some_and(|p| p.kind() == "binary_expression"
            && p.child_by_field_name("operator")
                .is_some_and(|o| o.utf8_text(source).unwrap_or("") == "+"))
    {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Don't concatenate `t()` results — use interpolation variables in the translation string instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(src, &Check)
    }

    #[test]
    fn flags_concat() {
        assert_eq!(run("const msg = t('hello') + ' ' + name").len(), 1);
    }

    #[test]
    fn allows_interpolation() {
        assert!(run("const msg = t('greeting', { name })").is_empty());
    }
}
