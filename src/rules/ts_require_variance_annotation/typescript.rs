//! Flags exported interfaces whose generic parameters lack `in`/`out`
//! variance modifiers.

use crate::diagnostic::{Diagnostic, Severity};

fn is_exported(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.kind() == "export_statement"
}

crate::ast_check! { on ["interface_declaration"] => |node, source, ctx, diagnostics|
    if !is_exported(node) {
        return;
    }
    let Some(type_params) = node.child_by_field_name("type_parameters") else { return };

    // tree-sitter-typescript does not model `in`/`out` variance modifiers, so
    // the grammar parses `<out T>` as a type_parameter named `out` followed by
    // an error. We work off the raw `<...>` text, splitting by commas and
    // checking whether each segment starts with `in`/`out`.
    let raw = std::str::from_utf8(&source[type_params.byte_range()]).unwrap_or("");
    let inner = raw.trim().trim_start_matches('<').trim_end_matches('>');
    if inner.trim().is_empty() {
        return;
    }
    let segments: Vec<&str> = inner.split(',').collect();
    for seg in &segments {
        let s = seg.trim();
        if s.is_empty() {
            continue;
        }
        let has_variance = s.starts_with("in ") || s.starts_with("out ");
        if !has_variance {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &type_params,
                super::META.id,
                format!("Generic parameter `{s}` needs an `in` or `out` variance annotation (exported interface)."),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_exported_interface_without_variance() {
        let diags = run("export interface Box<T> { value: T; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_exported_interface_with_out_variance() {
        assert!(run("export interface Box<out T> { value: T; }").is_empty());
    }

    #[test]
    fn allows_non_exported_interface() {
        assert!(run("interface Box<T> { value: T; }").is_empty());
    }

    #[test]
    fn allows_interface_without_generics() {
        assert!(run("export interface Plain { x: number; }").is_empty());
    }
}
