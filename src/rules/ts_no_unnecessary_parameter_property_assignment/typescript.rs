//! ts-no-unnecessary-parameter-property-assignment backend — flag
//! `this.x = x` inside constructors when `x` is a parameter property
//! (has an accessibility modifier like `public`, `private`, etc.).
//!
//! Detection: walk `assignment_expression` nodes inside constructors,
//! check if `this.X = X` where X is a parameter property.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "assignment_expression" {
        return;
    }
    // assignment_expression is always `=`. Compound operators like `+=`
    // use `augmented_assignment_expression` in tree-sitter-typescript.
    // Left side must be `this.something`
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    if left.kind() != "member_expression" {
        return;
    }
    let Some(obj) = left.child_by_field_name("object") else {
        return;
    };
    if obj.kind() != "this" {
        return;
    }
    let Some(prop) = left.child_by_field_name("property") else {
        return;
    };
    if prop.kind() != "property_identifier" {
        return;
    }
    let prop_name = &source[prop.byte_range()];
    // Right side must be a simple identifier matching the property name
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    if right.kind() != "identifier" {
        return;
    }
    let right_name = &source[right.byte_range()];
    if prop_name != right_name {
        return;
    }
    // Walk up to find the enclosing method_definition constructor
    let mut current = node.parent();
    let mut ctor_node = None;
    while let Some(p) = current {
        if p.kind() == "method_definition" {
            if let Some(name) = p.child_by_field_name("name")
                && &source[name.byte_range()] == b"constructor" {
                    ctor_node = Some(p);
                }
            break;
        }
        // Don't cross function boundaries
        if p.kind() == "function_declaration"
            || p.kind() == "function"
            || p.kind() == "arrow_function"
        {
            break;
        }
        current = p.parent();
    }
    let Some(ctor) = ctor_node else {
        return;
    };
    // Check if the matching parameter is a parameter property
    let Some(params) = ctor.child_by_field_name("parameters") else {
        return;
    };
    let mut pc = params.walk();
    let Ok(right_str) = std::str::from_utf8(right_name) else {
        return;
    };
    let right_str = right_str.trim();
    let mut is_param_property = false;
    for param in params.named_children(&mut pc) {
        if param.kind() != "required_parameter" {
            continue;
        }
        // Check for accessibility modifier
        let mut has_access = false;
        let mut param_name: Option<String> = None;
        let mut cc = param.walk();
        for child in param.children(&mut cc) {
            if child.kind() == "accessibility_modifier" || child.kind() == "readonly" {
                has_access = true;
            }
            if child.kind() == "identifier" && param_name.is_none()
                && let Ok(n) = std::str::from_utf8(&source[child.byte_range()]) {
                    param_name = Some(n.trim().to_string());
                }
        }
        if has_access
            && let Some(ref pn) = param_name
                && pn == right_str {
                    is_param_property = true;
                    break;
                }
    }
    if !is_param_property {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-unnecessary-parameter-property-assignment".into(),
        message: "This assignment is unnecessary — the parameter property already assigns it."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_redundant_assignment() {
        let src = r#"
class Foo {
    constructor(public name: string) {
        this.name = name;
    }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_different_property() {
        let src = r#"
class Foo {
    constructor(public name: string) {
        this.label = name;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_parameter_property() {
        let src = r#"
class Foo {
    constructor(name: string) {
        this.name = name;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
