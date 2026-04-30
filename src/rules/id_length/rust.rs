//! id-length Rust backend — flags `let`, function-parameter, and
//! struct-field bindings whose name is shorter than `min`.
//!
//! Usages and references are left alone — we only care about the
//! positions where the developer picked the name.

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier", "type_identifier", "field_identifier"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min = ctx.config.threshold("id-length", "min");
        let exceptions = ctx.config.string_list("id-length", "exceptions");
        let patterns = compile_patterns(&ctx.config.string_list("id-length", "exception_patterns"));

        let source_bytes = ctx.source.as_bytes();
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
        if is_sort_pair_param(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "id-length".into(),
            message: format!("Identifier `{name}` is too short (< {min})."),
            severity: Severity::Error,
            span: None,
        });
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
        "let_declaration" => field_matches(parent, "pattern", node),
        "parameter" => field_matches(parent, "pattern", node),
        "closure_parameters" => true,
        "for_expression" => field_matches(parent, "pattern", node),
        "if_let_expression" | "match_arm" => false,
        "function_item" | "const_item" | "static_item" | "struct_item" | "enum_item"
        | "trait_item" | "type_item" | "union_item" | "enum_variant" => {
            field_matches(parent, "name", node)
        }
        "field_declaration" => field_matches(parent, "name", node),
        _ => false,
    }
}

/// Allow `a` and `b` only when they are in a function/closure with exactly
/// 2 parameters both named `a` and `b` (sort/compare pattern).
fn is_sort_pair_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "parameter" {
        return false;
    }
    let Ok(name) = node.utf8_text(source) else {
        return false;
    };
    if name != "a" && name != "b" {
        return false;
    }
    let Some(func) = parent.parent() else {
        return false;
    };
    if func.kind() != "parameters" && func.kind() != "closure_parameters" {
        return false;
    }
    let param_names: Vec<&str> = (0..func.named_child_count())
        .filter_map(|i| {
            let child = func.named_child(i)?;
            if child.kind() != "parameter" {
                return None;
            }
            child.child_by_field_name("pattern")?.utf8_text(source).ok()
        })
        .collect();
    param_names.len() == 2 && param_names.contains(&"a") && param_names.contains(&"b")
}

fn field_matches(parent: tree_sitter::Node, field: &str, node: tree_sitter::Node) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|f| f.byte_range() == node.byte_range())
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_short_let_binding() {
        let diags = run_on("fn main() { let q = 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
    }

    #[test]
    fn flags_short_function_parameter() {
        let diags = run_on("fn g(q: u32) -> u32 { q }");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_short_struct_field() {
        let diags = run_on("struct S { q: u32 }");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_single_letter_names() {
        assert!(!run_on("fn main() { let f = File::open(\"x\"); }").is_empty());
        assert!(!run_on("fn main() { let s = String::new(); }").is_empty());
        assert!(!run_on("fn main() { let v = Vec::new(); }").is_empty());
        assert!(!run_on("fn main() { let n = 42; }").is_empty());
    }

    #[test]
    fn flags_closure_params() {
        assert!(!run_on("fn main() { vec![1].iter().map(|x| x + 1); }").is_empty());
    }

    #[test]
    fn flags_for_loop_var() {
        assert!(!run_on("fn main() { for i in 0..10 { println!(\"{}\", i); } }").is_empty());
    }

    #[test]
    fn allows_sort_pair_ab() {
        assert!(run_on("fn cmp(a: &i32, b: &i32) -> bool { a > b }").is_empty());
    }

    #[test]
    fn allows_closure_sort_pair_ab() {
        assert!(run_on("fn main() { vec![1].sort_by(|a: &i32, b: &i32| a.cmp(b)); }").is_empty());
    }

    #[test]
    fn flags_a_alone() {
        let diags = run_on("fn process(a: i32) -> i32 { a }");
        assert!(diags.iter().any(|d| d.message.contains("`a`")));
    }

    #[test]
    fn flags_ab_with_third_param() {
        let diags = run_on("fn process(a: i32, b: i32, c: i32) -> i32 { a + b + c }");
        assert!(diags.iter().any(|d| d.message.contains("`a`")));
        assert!(diags.iter().any(|d| d.message.contains("`b`")));
    }

    #[test]
    fn allows_long_names() {
        assert!(run_on("fn main() { let name = 1; }").is_empty());
    }

    #[test]
    fn does_not_flag_usage_only_references() {
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
        let diags = run_on("fn main() { let foo = 1; let q = 2; }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Identifier `q` is too short (< 2).");
    }
}
