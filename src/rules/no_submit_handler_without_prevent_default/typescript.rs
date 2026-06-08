//! no-submit-handler-without-preventDefault AST backend.
//!
//! Inspect JSX attributes named `onSubmit`. If the value is an inline
//! arrow function / function expression, walk its body and ensure a
//! `preventDefault()` call appears somewhere.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Extract the first parameter name from an arrow/function expression.
fn event_param_name<'a>(handler: Node, source: &'a [u8]) -> Option<&'a str> {
    let params = handler.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "identifier" => return child.utf8_text(source).ok(),
            "required_parameter" | "optional_parameter" => {
                let pat = child.child_by_field_name("pattern")?;
                if pat.kind() == "identifier" {
                    return pat.utf8_text(source).ok();
                }
            }
            _ => {}
        }
    }
    None
}

fn body_calls_prevent_default(body: Node, source: &[u8], param_name: &str) -> bool {
    let mut cursor = body.walk();
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        // Don't descend into nested functions.
        if node != body
            && matches!(
                node.kind(),
                "arrow_function" | "function_expression" | "function_declaration" | "function"
            )
        {
            continue;
        }
        if node.kind() == "call_expression"
            && let Some(func) = node.child_by_field_name("function")
            && func.kind() == "member_expression"
            && let Some(prop) = func.child_by_field_name("property")
            && let Ok(prop_text) = prop.utf8_text(source)
            && prop_text == "preventDefault"
            && let Some(obj) = func.child_by_field_name("object")
            && obj.kind() == "identifier"
            && let Ok(obj_text) = obj.utf8_text(source)
            && obj_text == param_name
        {
            return true;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn unwrap_jsx_expression(value: Node) -> Option<Node> {
    if value.kind() != "jsx_expression" {
        return None;
    }
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        if !matches!(child.kind(), "{" | "}") {
            return Some(child);
        }
    }
    None
}

crate::ast_check! { on ["jsx_attribute"] prefilter = ["onSubmit"] => |node, source, ctx, diagnostics|
    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if attr_name != "onSubmit" {
        return;
    }

    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    let Some(expr) = unwrap_jsx_expression(value) else { return };

    // Only inspect inline handlers; referenced identifiers are out of scope.
    let body = match expr.kind() {
        "arrow_function" | "function" | "function_expression" => {
            let Some(b) = expr.child_by_field_name("body") else { return };
            b
        }
        _ => return,
    };

    let Some(param_name) = event_param_name(expr, source) else { return };

    if body_calls_prevent_default(body, source, param_name) {
        return;
    }

    let pos = expr.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`onSubmit` handler does not call `preventDefault()` — the browser will perform a full-page submit and reset the form.".into(),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_arrow_without_prevent_default() {
        let src = "const x = <form onSubmit={(e) => submit(e)}>ok</form>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_expression_without_prevent_default() {
        let src = "const x = <form onSubmit={function (e) { submit(e); }}>ok</form>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_arrow_with_prevent_default() {
        let src = "const x = <form onSubmit={(e) => { e.preventDefault(); submit(e); }}>ok</form>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nested_prevent_default() {
        let src = r#"
const x = <form onSubmit={(e) => {
  if (valid) {
    e.preventDefault();
    submit(e);
  }
}}>ok</form>;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wrong_receiver() {
        let src = r#"const x = <form onSubmit={(event) => { other.preventDefault(); submit(event); }}>ok</form>;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_nested_function() {
        let src = r#"const x = <form onSubmit={(event) => { const f = () => event.preventDefault(); submit(event); }}>ok</form>;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_short_param_name() {
        let src =
            r#"const x = <form onSubmit={(e) => { e.preventDefault(); submit(e); }}>ok</form>;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_referenced_handler() {
        // Cannot easily track across scopes; keep to inline handlers.
        let src = "const x = <form onSubmit={handleSubmit}>ok</form>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_other_attributes() {
        let src = "const x = <button onClick={(e) => submit(e)}>ok</button>;";
        assert!(run_on(src).is_empty());
    }
}
