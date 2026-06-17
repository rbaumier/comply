//! no-weak-keys backend for Rust.
//!
//! Flags weak RSA key sizes (< 2048 bits) when a weak-length integer literal
//! sits in an actual crypto-key AST position:
//!
//! - it is a direct argument of a call whose callee's last path segment names a
//!   key-generation API (`Rsa::generate(512)`, `generate_key(1024)`), or
//! - it is the initializer of a `let`/`const`/`static` binding whose name is a
//!   key-length identifier (`let key_size = 1024;`).
//!
//! Matching is on tree-sitter nodes, never on the raw line text, so accessor
//! methods (`ty.bits()`), SIMD widths (`lane_count() == 256`), and other
//! integers that merely share a line with a keyword do not flag.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// RSA key lengths considered weak.
const WEAK_RSA_LENGTHS: &[&str] = &["256", "384", "512", "768", "1024"];

/// Last path segments of calls that denote a crypto key-generation API. Matched
/// against the callee's final `::` segment, so `Rsa::generate`,
/// `openssl::rsa::Rsa::generate`, and bare `generate` all qualify.
const KEYGEN_CALLEES: &[&str] = &[
    "generate",
    "generate_with_e",
    "generate_key",
    "generate_keypair",
    "new_rsa",
];

/// Binding names that denote an RSA key length. A literal initialized into a
/// binding with one of these *exact* names is a key length. Bare `bits` is
/// deliberately excluded: `let bits = 512;` is a width, not a key.
const KEY_LENGTH_BINDINGS: &[&str] = &[
    "key_size",
    "keysize",
    "key_len",
    "keylen",
    "key_length",
    "key_bits",
    "keybits",
    "rsa_bits",
    "rsa_key_size",
    "modulus",
    "modulus_bits",
];

crate::ast_check! { on ["integer_literal"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    if !WEAK_RSA_LENGTHS.contains(&text) {
        return;
    }

    if !is_keygen_call_argument(node, source) && !is_key_length_binding_value(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-weak-keys".into(),
        message: format!("Weak RSA key length ({text} bits) — use at least 2048 bits."),
        severity: Severity::Error,
        span: None,
    });
}

/// True if `literal` is a direct argument of a `call_expression` whose callee's
/// last path segment names a key-generation API (see `KEYGEN_CALLEES`).
///
/// The literal must be a named child of the call's `arguments` node — a macro
/// argument (`debug_assert!(.. <= 512)`, whose literal lives in a `token_tree`)
/// never qualifies, because a macro invocation is not a `call_expression`.
fn is_keygen_call_argument(literal: Node, source: &[u8]) -> bool {
    let Some(arguments) = literal.parent() else {
        return false;
    };
    if arguments.kind() != "arguments" {
        return false;
    }
    let Some(call) = arguments.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    let Ok(name) = callee.utf8_text(source) else {
        return false;
    };
    let tail = name.rsplit("::").next().unwrap_or(name).trim();
    KEYGEN_CALLEES.contains(&tail)
}

/// True if `literal` is the initializer (`value` field) of a `let_declaration`,
/// `const_item`, or `static_item` whose bound name is exactly a key-length
/// identifier (see `KEY_LENGTH_BINDINGS`), matched case-insensitively.
///
/// The name comes from the binding's pattern/name node, so `let key_size = 1024`
/// flags while `let bits = 512` does not. Names are compared whole, never as
/// substrings, so `.bits()` and `lane_count` cannot match.
fn is_key_length_binding_value(literal: Node, source: &[u8]) -> bool {
    let Some(binding) = literal.parent() else {
        return false;
    };
    let name_field = match binding.kind() {
        "let_declaration" => "pattern",
        "const_item" | "static_item" => "name",
        _ => return false,
    };
    // The literal must be the bound value, not some other child of the binding.
    if binding.child_by_field_name("value").map(|v| v.id()) != Some(literal.id()) {
        return false;
    }
    let Some(name_node) = binding.child_by_field_name(name_field) else {
        return false;
    };
    if name_node.kind() != "identifier" {
        return false;
    }
    let Ok(name) = name_node.utf8_text(source) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    KEY_LENGTH_BINDINGS.contains(&name.as_str())
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_rsa_1024() {
        assert_eq!(run_on("fn f() { let key_size = 1024; }").len(), 1);
    }

    #[test]
    fn flags_rsa_512() {
        assert_eq!(run_on("fn f() { Rsa::generate(512).unwrap(); }").len(), 1);
    }

    #[test]
    fn flags_key_bits_binding() {
        assert_eq!(run_on("fn f() { let key_bits = 1024; }").len(), 1);
    }

    #[test]
    fn flags_const_key_size_binding() {
        assert_eq!(run_on("const KEY_SIZE: u32 = 1024;").len(), 1);
    }

    #[test]
    fn allows_rsa_2048() {
        assert!(run_on("fn f() { let key_size = 2048; }").is_empty());
    }

    #[test]
    fn allows_non_key_integer() {
        assert!(run_on("fn f() { let port = 1024; }").is_empty());
    }

    // Regression for #3975: SIMD widths / `.bits()` accessors must not flag.
    #[test]
    fn allows_bits_accessor_in_debug_assert() {
        assert!(run_on("fn f() { debug_assert!(ty.bits() <= 512); }").is_empty());
    }

    #[test]
    fn allows_bits_accessor_in_if() {
        assert!(run_on("fn f() { if self.bits() > 256 { return None; } }").is_empty());
    }

    #[test]
    fn allows_lane_count_in_assert_eq() {
        assert!(run_on("fn f() { assert_eq!(big.lane_count(), 256); }").is_empty());
    }

    #[test]
    fn allows_plain_bits_binding() {
        assert!(run_on("fn f() { let bits = 512; }").is_empty());
    }
}
