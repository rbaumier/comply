//! no-class-inheritance backend — flag `class Foo extends Bar`.
//!
//! Exception: extending Error types is allowed (Error, CustomError, TaggedError, etc.)

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["class_declaration", "class"] => |node, source, ctx, diagnostics|
    // Look for a class_heritage child (the `extends` clause).
    let mut cursor = node.walk();
    let mut heritage_node = None;
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            heritage_node = Some(child);
            break;
        }
    }

    let heritage = match heritage_node {
        Some(h) => h,
        None => return,
    };

    // Extract the parent class name from the extends clause.
    // Structure: class_heritage > extends_clause > identifier (or member_expression)
    let mut hcursor = heritage.walk();
    for child in heritage.children(&mut hcursor) {
        if child.kind() == "extends_clause" {
            let mut ecursor = child.walk();
            for extends_child in child.children(&mut ecursor) {
                let parent_name = match extends_child.kind() {
                    "identifier" => extends_child.utf8_text(source).unwrap_or(""),
                    "member_expression" => extends_child.utf8_text(source).unwrap_or(""),
                    _ => continue,
                };
                // Allow extending Error types (Error, CustomError, TaggedError, etc.)
                if parent_name.to_lowercase().contains("error") {
                    return;
                }
            }
        }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-class-inheritance".into(),
        message: "Class inheritance via `extends` — prefer composition over inheritance.".into(),
        severity: Severity::Warning,
        span: None,
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

    #[test]
    fn allows_extends_error() {
        assert!(run_on("class MyError extends Error {}").is_empty());
    }

    #[test]
    fn allows_extends_custom_error() {
        assert!(run_on("class ValidationError extends CustomError {}").is_empty());
    }

    #[test]
    fn allows_extends_tagged_error() {
        assert!(run_on("class ApiError extends TaggedError {}").is_empty());
    }

    #[test]
    fn allows_extends_base_error() {
        assert!(run_on("class NotFoundError extends BaseError {}").is_empty());
    }
}
