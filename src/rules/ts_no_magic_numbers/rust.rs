//! no-magic-numbers (Rust) — flag integer/float literals that are not
//! in the common-constants allowlist and are not the RHS of a `const`
//! declaration, `static` declaration, or an `enum` discriminant.

use crate::diagnostic::{Diagnostic, Severity};

const ALLOWED: &[&str] = &[
    "-1", "0", "1", "2", "3", "4", "8", "16", "32", "64", "128", "256", "512", "1024",
    "0.0", "0.5", "1.0", "2.0", "0.", "1.", "2.",
    "0x00", "0xff", "0xFF", "0x0f", "0x0F",
];

const SUFFIXES: &[&str] = &[
    "usize", "isize", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "f32",
    "f64",
];

fn strip_suffix(text: &str) -> &str {
    let t = text.trim();
    for s in SUFFIXES {
        if let Some(stripped) = t.strip_suffix(s) {
            return stripped.trim_end_matches('_');
        }
    }
    t
}

fn is_allowed(text: &str) -> bool {
    let n = strip_suffix(text);
    ALLOWED.contains(&n)
}

fn is_in_skip_context(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    let mut depth = 0;
    while let Some(parent) = cur {
        match parent.kind() {
            "const_item" | "static_item" => {
                if depth <= 2 {
                    return true;
                }
            }
            "array_type" | "const_block" => return true,
            "enum_variant" => return true,
            "let_declaration" => return true,
            "match_pattern" | "match_arm" => return true,
            "range_expression" => return true,
            "macro_invocation" => return true,
            "token_tree" => return true,
            "attribute_item" | "attribute" => return true,
            "binary_expression" => {
                if let Some(op) = parent.child_by_field_name("operator") {
                    let op_text = op.utf8_text(source).unwrap_or("");
                    if matches!(op_text, "<<" | ">>" | "&" | "|" | "^") {
                        return true;
                    }
                }
            }
            _ => {}
        }
        cur = parent.parent();
        depth += 1;
    }
    false
}

crate::ast_check! { on ["integer_literal", "float_literal"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if ctx.path.to_string_lossy().contains("/examples/") { return; }
    if crate::rules::rust_helpers::is_in_test_context(node, source) { return; }
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    if is_allowed(text) {
        return;
    }
    if is_in_skip_context(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Magic number `{text}` — extract it into a named `const`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_inline_magic() {
        // `len * 3600` has a magic 3600.
        assert_eq!(run_on("fn f(len: u64) -> u64 { len * 3600 }").len(), 1);
    }

    #[test]
    fn flags_magic_in_call() {
        assert_eq!(run_on("fn f() { g(8080); } fn g(_: u32) {}").len(), 1);
    }

    #[test]
    fn allows_const_item() {
        assert!(run_on("const PORT: u32 = 8080;").is_empty());
    }

    #[test]
    fn allows_allowlist() {
        assert!(run_on("fn f() -> i32 { 0 + 1 + 2 }").is_empty());
    }

    #[test]
    fn allows_array_size_type() {
        assert!(run_on("fn f() -> [u8; 4] { [0, 0, 0, 0] }").is_empty());
    }

    #[test]
    fn allows_zero_with_suffix() {
        assert!(run_on("fn f() -> usize { 0usize }").is_empty());
        assert!(run_on("fn f() -> f32 { 0f32 }").is_empty());
        assert!(run_on("fn f() -> i64 { 1i64 }").is_empty());
    }

    #[test]
    fn flags_magic_with_suffix() {
        assert_eq!(run_on("fn f() -> usize { 42usize }").len(), 1);
        assert_eq!(run_on("fn f() -> f64 { 3.14f64 }").len(), 1);
    }

    #[test]
    fn allows_float_equivalents_of_allowed_integers() {
        assert!(run_on("fn f() -> f32 { 0.0 }").is_empty());
        assert!(run_on("fn f() -> f64 { 1.0 }").is_empty());
        assert!(run_on("fn f() -> f32 { 2.0 }").is_empty());
        assert!(run_on("fn f() -> f32 { 0. }").is_empty());
        assert!(run_on("fn f() -> f32 { 1. }").is_empty());
    }

    #[test]
    fn allows_let_binding() {
        assert!(run_on("fn f() { let limit = 3600; }").is_empty());
    }

    #[test]
    fn allows_match_arm() {
        assert!(run_on("fn f(x: u32) { match x { 42 => {}, _ => {} } }").is_empty());
    }

    #[test]
    fn allows_bit_operations() {
        assert!(run_on("fn f(x: u32) -> u32 { x << 3 }").is_empty());
        assert!(run_on("fn f(x: u32) -> u32 { x & 0xFF }").is_empty());
    }

    #[test]
    fn allows_range_expression() {
        assert!(run_on("fn f() { for i in 0..100 {} }").is_empty());
    }

    #[test]
    fn allows_macro_invocations() {
        assert!(run_on("fn f() { assert_eq!(x, 42); }").is_empty());
        assert!(run_on("fn f() { vec![0; 64]; }").is_empty());
    }

    #[test]
    fn allows_common_powers_of_two() {
        assert!(run_on("fn f(x: u32) -> bool { x >= 32 }").is_empty());
        assert!(run_on("fn f() -> usize { 1024 }").is_empty());
    }

    #[test]
    fn allows_attribute_numbers() {
        assert!(run_on("#[repr(align(16))]\nstruct Foo { x: u8 }").is_empty());
    }
}
