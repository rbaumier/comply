//! jsx-handler-names backend — flag JSX event handler props wired to
//! bare identifiers whose name does not start with `handle`, `on`, or
//! `toggle`.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `name` looks like an event-handler prop: `on` followed by an
/// uppercase letter (e.g. `onClick`, `onSubmit`). Plain `on` or
/// `only` are excluded.
fn is_event_handler_prop(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || &bytes[..2] != b"on" {
        return false;
    }
    bytes[2].is_ascii_uppercase()
}

/// True if the identifier name starts with an accepted handler prefix.
fn has_valid_handler_prefix(name: &str) -> bool {
    let prefixes: [&str; 3] = ["handle", "on", "toggle"];
    prefixes.iter().any(|p| {
        if let Some(rest) = name.strip_prefix(p) {
            rest.as_bytes()
                .first()
                .is_none_or(|b| b.is_ascii_uppercase())
        } else {
            false
        }
    })
}

/// Walk a `jsx_expression` and return its single inner expression, if any.
fn jsx_expression_inner(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "{" | "}" | "comment" => continue,
            _ => return Some(child),
        }
    }
    None
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if !is_event_handler_prop(attr_name) {
        return;
    }
    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value.kind() != "jsx_expression" {
        return;
    }
    let Some(inner) = jsx_expression_inner(value) else { return };

    // Inline functions, calls, and member expressions are all fine.
    match inner.kind() {
        "arrow_function"
        | "function_expression"
        | "function"
        | "call_expression"
        | "member_expression" => return,
        "identifier" => {}
        _ => return,
    }

    let Ok(ident) = inner.utf8_text(source) else { return };
    if has_valid_handler_prefix(ident) {
        return;
    }
    let pos = inner.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "jsx-handler-names".into(),
        message: format!(
            "Handler `{ident}` passed to `{attr_name}` should be named `handle*`, `on*`, or `toggle*`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_bad_prefix_identifier() {
        let d = run_on("const x = <Button onClick={doStuff} />;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_handle_prefix() {
        assert!(run_on("const x = <Button onClick={handleClick} />;").is_empty());
    }

    #[test]
    fn allows_on_prefix() {
        assert!(run_on("const x = <Button onClick={onSubmit} />;").is_empty());
    }

    #[test]
    fn allows_toggle_prefix() {
        assert!(run_on("const x = <Button onClick={toggleMenu} />;").is_empty());
    }

    #[test]
    fn allows_inline_arrow() {
        assert!(run_on("const x = <Button onClick={() => {}} />;").is_empty());
    }

    #[test]
    fn allows_member_expression() {
        assert!(run_on("const x = <Button onClick={props.onClick} />;").is_empty());
    }

    #[test]
    fn allows_call_expression() {
        assert!(run_on("const x = <Button onClick={makeHandler()} />;").is_empty());
    }

    #[test]
    fn ignores_non_handler_prop() {
        assert!(run_on("const x = <Button label={doStuff} />;").is_empty());
    }

    #[test]
    fn ignores_lowercase_on_prefix_prop() {
        // `only` shouldn't be treated as an event handler.
        assert!(run_on("const x = <Foo only={doStuff} />;").is_empty());
    }
}
