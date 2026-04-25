//! zod-require-error-messages backend — flag `.refine(fn)` calls that
//! omit the second argument carrying the error message.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }

    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "member_expression" { return; }

    let Some(property) = function.child_by_field_name("property") else { return };
    if property.utf8_text(source).ok() != Some("refine") { return; }

    let Some(arguments) = node.child_by_field_name("arguments") else { return };

    // Count top-level argument children (skip punctuation).
    let mut arg_count = 0usize;
    let mut cursor = arguments.walk();
    for child in arguments.children(&mut cursor) {
        if child.is_named() {
            arg_count += 1;
        }
    }
    if arg_count >= 2 { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `{ message: '...' }` to `.refine()` — bare refine produces no helpful error message.".into(),
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
    fn flags_single_arg_refine() {
        assert_eq!(run("z.string().refine(val => val.includes('@'))").len(), 1);
    }

    #[test]
    fn allows_refine_with_message() {
        assert!(
            run("z.string().refine(val => val.includes('@'), { message: 'Must be email' })")
                .is_empty()
        );
    }
}
