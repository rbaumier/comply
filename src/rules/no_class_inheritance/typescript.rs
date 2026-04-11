//! no-class-inheritance backend — flag `class Foo extends Bar`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "class_declaration" && node.kind() != "class" {
        return;
    }

    // Look for a class_heritage child (the `extends` clause).
    let mut cursor = node.walk();
    let mut has_extends = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            has_extends = true;
            break;
        }
    }

    if !has_extends {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-class-inheritance".into(),
        message: "Class inheritance via `extends` — prefer composition over inheritance.".into(),
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
    fn flags_class_extends() {
        assert_eq!(run_on("class Dog extends Animal {}").len(), 1);
    }

    #[test]
    fn flags_export_class_extends() {
        assert_eq!(run_on("export class Foo extends Base {}").len(), 1);
    }

    #[test]
    fn allows_class_without_extends() {
        assert!(run_on("class Foo {}").is_empty());
    }

    #[test]
    fn allows_class_expression_without_extends() {
        assert!(run_on("const Foo = class {};").is_empty());
    }
}
