//! AST backend for react-no-boolean-variant-props.
//!
//! Detects function components (function/arrow) whose first parameter
//! is an object_pattern destructuring 2+ identifiers matching the
//! `is[A-Z]...` or `has[A-Z]...` naming convention.

use crate::diagnostic::{Diagnostic, Severity};

/// Independent observable state flags: a single bit of truth that can hold
/// simultaneously with the others (a form is dirty AND submitting AND
/// disabled). These are NOT mutually-exclusive variants and must not be
/// collapsed into a union. Style/intent variants (`isPrimary`, `isGhost`) and
/// request-status flags (`isLoading`, `isError`, `isSuccess`) are deliberately
/// absent — collapsing *those* is the boolean-blindness smell the rule targets.
const INDEPENDENT_OBSERVABLE_FLAGS: &[&str] = &[
    "Dirty", "Submitting", "Submitted", "Saving", "Saved", "Editing", "Open",
    "Opened", "Closed", "Visible", "Hidden", "Valid", "Invalid", "Checked",
    "Unchecked", "Selected", "Deselected", "Disabled", "Enabled", "Active",
    "Inactive", "Focused", "Blurred", "Touched", "Untouched", "Expanded",
    "Collapsed", "Hovered", "Pressed", "Dragging", "Animating", "ReadOnly",
    "Required", "Optional", "Mounted", "Ready", "Deleting",
];

fn looks_like_variant_prop(name: &str) -> bool {
    for prefix in ["is", "has"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return !INDEPENDENT_OBSERVABLE_FLAGS.contains(&rest);
        }
    }
    false
}

fn function_name_is_component(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn is_function_component(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match node.kind() {
        "function_declaration" => {
            let Some(name) = node.child_by_field_name("name") else {
                return false;
            };
            let Ok(text) = name.utf8_text(source) else {
                return false;
            };
            function_name_is_component(text)
        }
        "arrow_function" | "function_expression" => {
            // Check enclosing variable_declarator for a PascalCase id.
            let Some(parent) = node.parent() else {
                return false;
            };
            if parent.kind() != "variable_declarator" {
                return false;
            }
            let Some(name) = parent.child_by_field_name("name") else {
                return false;
            };
            let Ok(text) = name.utf8_text(source) else {
                return false;
            };
            function_name_is_component(text)
        }
        _ => false,
    }
}

fn first_param<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let params = node.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .find(|c| c.kind() != "comment")
}

fn count_boolean_variants(pattern: tree_sitter::Node<'_>, source: &[u8]) -> usize {
    if pattern.kind() != "object_pattern" {
        return 0;
    }
    let mut cursor = pattern.walk();
    let mut count = 0usize;
    for child in pattern.named_children(&mut cursor) {
        let name_node = match child.kind() {
            "shorthand_property_identifier_pattern" => Some(child),
            "pair_pattern" => child.child_by_field_name("key"),
            "object_assignment_pattern" => {
                // { isX = false } — left is the shorthand id.
                child.child_by_field_name("left")
            }
            _ => None,
        };
        let Some(n) = name_node else { continue };
        if let Ok(text) = n.utf8_text(source)
            && looks_like_variant_prop(text)
        {
            count += 1;
        }
    }
    count
}

crate::ast_check! { on ["function_declaration", "arrow_function", "function_expression"] => |node, source, ctx, diagnostics|
    let _ = ctx;
        if !is_function_component(node, source) {
        return;
    }
    let Some(param) = first_param(node) else { return };
    // Destructuring can appear directly (`{ isX }`) or inside a typed
    // param `{ isX }: Props`.
    let pattern = if param.kind() == "object_pattern" {
        param
    } else {
        let mut cursor = param.walk();
        let Some(p) = param
            .named_children(&mut cursor)
            .find(|c| c.kind() == "object_pattern")
        else {
            return;
        };
        p
    };
    let count = count_boolean_variants(pattern, source);
    if count < 2 {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &pattern,
        super::META.id,
        format!(
            "{count} boolean variant props on this component — collapse into a single \
             `variant: '...' | '...'` union to eliminate mutually-exclusive invalid states."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_two_boolean_variants() {
        let src = r#"function Button({ isPrimary, isGhost }) { return <button/>; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_two_boolean_variants_arrow() {
        let src = r#"const Button = ({ isPrimary, hasIcon }) => <button/>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_boolean_variant() {
        let src = r#"function Button({ isPrimary, label }) { return <button/>; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variant_union() {
        let src = r#"function Button({ variant }) { return <button/>; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component() {
        let src = r#"function helper({ isA, isB }) {}"#;
        assert!(run(src).is_empty());
    }
}
