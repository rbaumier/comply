//! no-inline-function-event-listener backend — flag `addEventListener('x', () => ...)`
//! where the callback is an inline arrow/function expression. Such listeners
//! cannot be removed via `removeEventListener` because the function reference
//! is not retained anywhere.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let Ok(property_text) = property.utf8_text(source) else { return };
    if property_text != "addEventListener" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named_args: Vec<_> = args.named_children(&mut cursor).collect();
    let Some(second) = named_args.get(1) else { return };
    let kind = second.kind();
    if kind != "arrow_function" && kind != "function_expression" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-inline-function-event-listener",
        "Inline function passed to addEventListener cannot be removed — extract to a named function for proper cleanup.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_inline_arrow() {
        assert_eq!(
            run_on("el.addEventListener('click', () => doThing())").len(),
            1
        );
    }

    #[test]
    fn flags_inline_function_expression() {
        assert_eq!(
            run_on("el.addEventListener('click', function () { doThing(); })").len(),
            1
        );
    }

    #[test]
    fn allows_named_identifier_reference() {
        assert!(
            run_on("el.addEventListener('click', handleClick)").is_empty()
        );
    }

    #[test]
    fn allows_member_expression_reference() {
        assert!(
            run_on("el.addEventListener('click', this.handleClick)").is_empty()
        );
    }

    #[test]
    fn ignores_non_addeventlistener_calls() {
        assert!(run_on("arr.forEach(() => doThing())").is_empty());
    }
}
