//! AST backend for react-no-setstate-without-updater.
//!
//! Detects the `[x, setX] = useState(...)` pair at the
//! `variable_declarator` level, then walks the enclosing function body
//! looking for `setX(expr)` calls whose argument (a) is not an arrow /
//! function expression and (b) references `x` as a free identifier.

use crate::diagnostic::{Diagnostic, Severity};

/// Return `(state_name, setter_name, declarator_node)` if this is a
/// `useState` destructuring declaration.
fn extract_usestate<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(String, String)> {
    if node.kind() != "variable_declarator" {
        return None;
    }
    let pattern = node.child_by_field_name("name")?;
    if pattern.kind() != "array_pattern" {
        return None;
    }
    let init = node.child_by_field_name("value")?;
    if init.kind() != "call_expression" {
        return None;
    }
    let callee = init.child_by_field_name("function")?;
    let callee_text = callee.utf8_text(source).ok()?;
    // Accept `useState` or `React.useState`.
    if callee_text != "useState" && !callee_text.ends_with(".useState") {
        return None;
    }
    let mut cursor = pattern.walk();
    let names: Vec<_> = pattern
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "identifier")
        .collect();
    if names.len() < 2 {
        return None;
    }
    let state = names[0].utf8_text(source).ok()?.to_string();
    let setter = names[1].utf8_text(source).ok()?.to_string();
    Some((state, setter))
}

fn argument_references(node: tree_sitter::Node<'_>, source: &[u8], name: &str) -> bool {
    // Skip nested arrow/function — those are the correct updater form.
    if node.kind() == "arrow_function" || node.kind() == "function_expression" {
        return false;
    }
    if node.kind() == "identifier" {
        if let Ok(text) = node.utf8_text(source)
            && text == name {
                return true;
            }
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if argument_references(child, source, name) {
            return true;
        }
    }
    false
}

fn scan_function_body<'a>(
    body: tree_sitter::Node<'a>,
    source: &[u8],
    state: &str,
    setter: &str,
    out: &mut Vec<tree_sitter::Node<'a>>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "call_expression"
            && let Some(callee) = child.child_by_field_name("function")
                && callee.kind() == "identifier"
                    && callee.utf8_text(source).ok() == Some(setter)
                    && let Some(args) = child.child_by_field_name("arguments") {
                        let mut arg_cursor = args.walk();
                        for arg in args.named_children(&mut arg_cursor) {
                            if argument_references(arg, source, state) {
                                out.push(child);
                                break;
                            }
                        }
                    }
        scan_function_body(child, source, state, setter, out);
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    let Some((state, setter)) = extract_usestate(node, source) else { return };
    // Find the enclosing function body.
    let mut scope = node.parent();
    while let Some(p) = scope {
        match p.kind() {
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition" => break,
            _ => scope = p.parent(),
        }
    }
    let Some(func) = scope else { return };
    let Some(body) = func.child_by_field_name("body") else { return };
    let mut bad = Vec::new();
    scan_function_body(body, source, &state, &setter, &mut bad);
    for call in bad {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &call,
            super::META.id,
            format!(
                "`{setter}` called with an expression referencing `{state}` — \
                 use the functional updater: `{setter}(prev => ...)`."
            ),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_increment_without_updater() {
        let src = r#"
function C() {
  const [count, setCount] = useState(0);
  const inc = () => setCount(count + 1);
  return <button onClick={inc} />;
}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_array_spread_without_updater() {
        let src = r#"
function C() {
  const [items, setItems] = useState([]);
  const add = (i) => setItems([...items, i]);
  return <button onClick={add} />;
}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_functional_updater() {
        let src = r#"
function C() {
  const [count, setCount] = useState(0);
  const inc = () => setCount(prev => prev + 1);
  return <button onClick={inc} />;
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_setter_with_unrelated_value() {
        let src = r#"
function C() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(42)} />;
}"#;
        assert!(run(src).is_empty());
    }
}
