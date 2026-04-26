//! react-prefer-use-transition — flag manual `useState(false)` loading
//! flags driven by `setLoading(true)` / `setLoading(false)` around an
//! `await`. Suggest `useTransition` instead so React can batch the
//! pending state with concurrent rendering.
//!
//! AST detection: walk `variable_declarator` nodes whose initializer is
//! `useState(false)` and whose name is an array_pattern with two
//! identifiers. Then verify the file actually awaits something and
//! calls `setter(true)` + `setter(false)`.

use crate::diagnostic::{Diagnostic, Severity};

fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn is_use_state_false(init: tree_sitter::Node, source: &[u8]) -> bool {
    if init.kind() != "call_expression" {
        return false;
    }
    let Some(func) = init.child_by_field_name("function") else {
        return false;
    };
    if node_text(func, source) != "useState" {
        return false;
    }
    let Some(args) = init.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if arg.kind() == "false" {
            return true;
        }
    }
    false
}

fn extract_setter_name<'a>(pattern: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    if pattern.kind() != "array_pattern" {
        return None;
    }
    let mut cursor = pattern.walk();
    let names: Vec<&str> = pattern
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "identifier" || c.kind() == "shorthand_property_identifier_pattern")
        .map(|c| node_text(c, source))
        .collect();
    if names.len() == 2 {
        Some(names[1])
    } else {
        None
    }
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    // Skip if file already uses useTransition.
    if ctx.source.contains("useTransition") {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Some(value_node) = node.child_by_field_name("value") else { return };
    if !is_use_state_false(value_node, source) {
        return;
    }
    let Some(setter) = extract_setter_name(name_node, source) else { return };
    if setter.is_empty() {
        return;
    }
    let src = ctx.source;
    if !src.contains(&format!("{setter}(true)"))
        || !src.contains(&format!("{setter}(false)"))
        || !src.contains("await ")
    {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Replace manual `{setter}(true/false)` loading state with `useTransition`."),
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
    fn flags_manual_loading_state() {
        let src = "const [loading, setLoading] = useState(false)\nasync function submit() { setLoading(true); await post(); setLoading(false) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_transition() {
        let src = "const [isPending, startTransition] = useTransition()\nconst [loading, setLoading] = useState(false)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_await() {
        let src = "const [loading, setLoading] = useState(false)\nfunction submit() { setLoading(true); post(); setLoading(false) }";
        assert!(run(src).is_empty());
    }
}
