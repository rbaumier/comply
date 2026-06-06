//! Classes that implement `PipeTransform` must declare a `transform` method.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "PipeTransform")
}

fn class_implements_pipe_transform(class: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = class.walk();
    for child in class.children(&mut cursor) {
        if child.kind() == "class_heritage" || child.kind() == "implements_clause" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if text.contains("PipeTransform") { return true; }
        }
    }
    false
}

fn class_has_transform_method(class: tree_sitter::Node, source: &[u8]) -> bool {
    let body = match class.child_by_field_name("body") { Some(b) => b, None => return false };
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "method_definition"
            && let Some(name) = child.child_by_field_name("name")
        {
            let text = std::str::from_utf8(&source[name.byte_range()]).unwrap_or("");
            if text == "transform" { return true; }
        }
    }
    false
}

crate::ast_check! { on ["class_declaration"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    if !class_implements_pipe_transform(node, source) { return; }
    if class_has_transform_method(node, source) { return; }
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Class `{name}` implements `PipeTransform` but is missing the required `transform()` method."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_pipe_without_transform() {
        let src = "import { PipeTransform } from '@nestjs/common';\nexport class P implements PipeTransform { other() {} }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pipe_with_transform() {
        let src = "import { PipeTransform } from '@nestjs/common';\nexport class P implements PipeTransform { transform(v: any) { return v; } }";
        assert!(run(src).is_empty());
    }
}
