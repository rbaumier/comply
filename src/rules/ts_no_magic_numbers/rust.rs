//! no-magic-numbers (Rust) — flag integer/float literals that are not
//! in the common-constants allowlist and are not the RHS of a `const`
//! declaration, `static` declaration, or an `enum` discriminant.

use crate::diagnostic::{Diagnostic, Severity};

const ALLOWED: &[&str] = &["0", "1", "2", "-1", "0.0", "1.0", "2.0", "0.", "1.", "2."];

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

fn is_in_skip_context(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    let mut depth = 0;
    while let Some(parent) = cur {
        match parent.kind() {
            // `const FOO: u32 = 42;` / `static BAR: u32 = 42;` — literal is being named.
            "const_item" | "static_item" => {
                if depth <= 2 {
                    return true;
                }
            }
            // Array repeat / type-level number (`[u8; 4]`).
            "array_type" | "const_block" => return true,
            // Enum discriminant `Foo = 42`.
            "enum_variant" => return true,
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
    if is_in_skip_context(node) {
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
}
