//! ts-default-param-last backend — walk function declarations/expressions/arrows
//! and flag default (or optional) parameters that are not at the end of the
//! parameter list.
//!
//! In tree-sitter-typescript, a default parameter like `a = 1` is a
//! `required_parameter` that contains a `value` field (the default expression).
//! An optional parameter is `optional_parameter`. We iterate from the end;
//! once we see a plain required param (no default), any default/optional
//! before it is flagged.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "function_declaration"
        && kind != "function"
        && kind != "arrow_function"
        && kind != "method_definition"
        && kind != "function_signature"
        && kind != "method_signature"
    {
        return;
    }

    let Some(params) = node.child_by_field_name("parameters") else {
        return;
    };

    let mut cursor = params.walk();
    let children: Vec<_> = params.named_children(&mut cursor).collect();

    // Walk from the end. Once we see a plain required param, all
    // default/optional params before it are violations.
    let mut seen_plain = false;
    for child in children.iter().rev() {
        let ck = child.kind();
        if ck == "rest_parameter" {
            continue;
        }

        let is_default = has_default_value(child);
        let is_optional = ck == "optional_parameter";

        if !is_default && !is_optional {
            seen_plain = true;
            continue;
        }

        if seen_plain {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-default-param-last".into(),
                message: "Default parameters should be last.".into(),
                severity: Severity::Warning,
            });
        }
    }
}

/// Check if a parameter node has a default value. In tree-sitter-typescript,
/// `required_parameter` with a default (`a = 1`) has a `value` field.
/// Also handles bare `assignment_pattern` nodes just in case.
fn has_default_value(node: &tree_sitter::Node) -> bool {
    // Check for `value` field (tree-sitter-typescript uses this for defaults).
    if node.child_by_field_name("value").is_some() {
        return true;
    }
    // Also check for `=` token among children (fallback).
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i)
            && child.kind() == "=" {
                return true;
            }
    }
    node.kind() == "assignment_pattern"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_default_param_before_required() {
        let diags = run_on("function foo(a = 1, b: number) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Default parameters"));
    }

    #[test]
    fn allows_default_param_last() {
        let diags = run_on("function foo(a: number, b = 1) {}");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_all_default_params() {
        let diags = run_on("function foo(a = 1, b = 2) {}");
        assert!(diags.is_empty());
    }
}
