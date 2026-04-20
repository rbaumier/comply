//! id-length Rust backend — flags `let`, function-parameter, and
//! struct-field bindings whose name is shorter than `min`.
//!
//! Usages and references are left alone — we only care about the
//! positions where the developer picked the name.

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let min = ctx.config.threshold("id-length", "min");
        let exceptions = ctx.config.string_list("id-length", "exceptions");
        let patterns = compile_patterns(&ctx.config.string_list("id-length", "exception_patterns"));

        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if !is_rust_binding_name(node) {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            if name.chars().count() >= min {
                return;
            }
            if exceptions.iter().any(|e| e == name) {
                return;
            }
            if patterns.iter().any(|p| p.is_match(name)) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "id-length".into(),
                message: format!("Identifier `{name}` is too short (< {min})."),
                severity: Severity::Error,
                span: None,
            });
        });

        diagnostics
    }
}

/// Binding positions in tree-sitter-rust:
///   - `let_declaration.pattern` → `identifier` (`let x = …`)
///   - `parameter.pattern` → `identifier` (`fn f(x: T)`)
///   - `function_item.name` → `identifier` (`fn f()`)
///   - `struct_item.name` / `enum_item.name` / `trait_item.name` / `type_item.name` → `type_identifier`
///   - `field_declaration.name` → `field_identifier` (`struct S { x: u8 }`)
///   - `const_item.name` / `static_item.name` → `identifier`
fn is_rust_binding_name(node: tree_sitter::Node) -> bool {
    let kind = node.kind();
    if kind != "identifier" && kind != "type_identifier" && kind != "field_identifier" {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let parent_kind = parent.kind();

    match parent_kind {
        "let_declaration" | "parameter" => field_matches(parent, "pattern", node),
        "function_item" | "const_item" | "static_item" | "struct_item" | "enum_item"
        | "trait_item" | "type_item" | "union_item" | "enum_variant" => {
            field_matches(parent, "name", node)
        }
        "field_declaration" => field_matches(parent, "name", node),
        _ => false,
    }
}

fn field_matches(parent: tree_sitter::Node, field: &str, node: tree_sitter::Node) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|f| f.byte_range() == node.byte_range())
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_short_let_binding() {
        let diags = run_on("fn main() { let x = 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`x`"));
    }

    #[test]
    fn flags_short_function_parameter() {
        let diags = run_on("fn f(x: u32) -> u32 { x }");
        // `f` (function name) + `x` (parameter)
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_short_struct_field() {
        let diags = run_on("struct S { x: u32 }");
        assert_eq!(diags.len(), 2, "S and x both < 2 chars");
    }

    #[test]
    fn allows_long_names() {
        assert!(run_on("fn main() { let name = 1; }").is_empty());
    }

    #[test]
    fn does_not_flag_usage_only_references() {
        // `foo(x)` where `x` is declared elsewhere should not re-flag
        // the reference.
        assert!(run_on("fn main() { foo(x); }").is_empty());
    }

    #[test]
    fn flags_short_const_name() {
        let diags = run_on("const N: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`N`"));
    }

    #[test]
    fn message_names_the_identifier() {
        let diags = run_on("fn main() { let foo = 1; let x = 2; }");
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "Identifier `x` is too short (< 2)."
        );
    }
}
