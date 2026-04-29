//! no-invalid-remove-event-listener AST backend — flag `removeEventListener`
//! whose listener argument is an inline function expression / arrow function
//! or a `.bind(...)` call. These create a fresh function reference at every
//! call site, so the listener will never actually be removed.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `callee` is a member expression `<x>.removeEventListener`.
fn is_remove_listener(callee: tree_sitter::Node, source: &[u8]) -> bool {
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).unwrap_or("") == "removeEventListener"
}

/// True if `arg` is an inline function expression / arrow / `.bind(...)`
/// call — i.e. a value that creates a fresh reference at the call site.
fn is_inline_listener(arg: tree_sitter::Node, source: &[u8]) -> bool {
    match arg.kind() {
        "arrow_function" | "function_expression" | "function" => true,
        "call_expression" => {
            let Some(callee) = arg.child_by_field_name("function") else {
                return false;
            };
            if callee.kind() != "member_expression" {
                return false;
            }
            let Some(prop) = callee.child_by_field_name("property") else {
                return false;
            };
            prop.utf8_text(source).unwrap_or("") == "bind"
        }
        _ => false,
    }
}

crate::ast_check! { on ["call_expression"] prefilter = ["removeEventListener"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !is_remove_listener(callee, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(listener) = args.named_children(&mut cursor).nth(1) else { return };
    if !is_inline_listener(listener, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-invalid-remove-event-listener",
        "The listener argument should be a function reference — inline functions and `.bind()` create a new reference each call.".into(),
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
    fn flags_bind_call() {
        let code = r#"el.removeEventListener('click', handler.bind(this));"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_arrow_function() {
        let code = r#"el.removeEventListener('click', () => handler());"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_function_expression() {
        let code = r#"el.removeEventListener('click', function() { handler(); });"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_function_reference() {
        let code = r#"el.removeEventListener('click', handler);"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_variable_reference() {
        let code = r#"el.removeEventListener('click', this.onClickBound);"#;
        assert!(run_on(code).is_empty());
    }
}
