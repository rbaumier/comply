//! Flag `class FooService { ... }` (or `*Repository` / `*UseCase`) without
//! `@Injectable()` decorator in NestJS files.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    source.contains("@nestjs/")
}

fn class_name_looks_like_provider(name: &str) -> bool {
    name.ends_with("Service") || name.ends_with("Repository") || name.ends_with("UseCase") || name.ends_with("Provider")
}

fn class_has_injectable_decorator(class: tree_sitter::Node, source: &[u8]) -> bool {
    // Decorators are children of the class_declaration in the TS grammar, OR
    // siblings inside an export_statement.
    let mut cursor = class.walk();
    for child in class.children(&mut cursor) {
        if child.kind() == "decorator" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if text.contains("@Injectable") { return true; }
        }
    }
    if let Some(parent) = class.parent() {
        let mut cur = parent.walk();
        for child in parent.children(&mut cur) {
            if child.kind() == "decorator" {
                let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
                if text.contains("@Injectable") { return true; }
            }
        }
    }
    false
}

crate::ast_check! { on ["class_declaration"] prefilter = ["@nestjs/"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    if !class_name_looks_like_provider(name) { return; }
    if class_has_injectable_decorator(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Class `{name}` looks like a NestJS provider but is missing `@Injectable()`."),
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
    fn flags_service_without_injectable() {
        let src = "import { Module } from '@nestjs/common';\nexport class UserService {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_service_with_injectable() {
        let src = "import { Injectable } from '@nestjs/common';\n@Injectable() export class UserService {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_provider_class() {
        let src = "import { Module } from '@nestjs/common';\nexport class UserDto {}";
        assert!(run(src).is_empty());
    }
}
