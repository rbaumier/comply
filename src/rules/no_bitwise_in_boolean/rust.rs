//! no-bitwise-in-boolean Rust backend.
//!
//! Flag bitwise ops (`&`, `|`) in boolean contexts (if/while conditions). `^` is
//! not flagged: it is the only way to express logical XOR on `bool` in Rust.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{bitop_operand_type_has_bool_output_impl, operand_is_bool};

// `^` is excluded: `bool ^ bool` is the only way to express logical XOR in Rust
// (there is no `^^`), and a `^` reached in a boolean condition is always that
// idiom — `int ^ int` yields an int, which is not a valid condition. `&`/`|`
// keep their short-circuit analogues (`&&`/`||`), so the typo warning applies.
const BITWISE_OPS: &[&str] = &["&", "|"];

const COMPARISON_OPS: &[&str] = &["==", "!=", "<", ">", "<=", ">="];

fn has_bitwise_op(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "binary_expression" => {
            if let Some(op) = node.child_by_field_name("operator") {
                let op_text = op.utf8_text(source).unwrap_or("");
                if COMPARISON_OPS.contains(&op_text) {
                    return false;
                }
                if BITWISE_OPS.contains(&op_text) {
                    // `bool & bool` / `bool | bool` is Rust's branchless,
                    // non-short-circuit logical AND/OR — type-safe and idiomatic,
                    // not a `&&`/`||` typo. Suppress only when both operands are
                    // provably boolean; a non-bool operand keeps firing.
                    let left = node.child_by_field_name("left");
                    let right = node.child_by_field_name("right");
                    let both_bool = left.zip(right).is_some_and(|(l, r)| {
                        operand_is_bool(l, source) && operand_is_bool(r, source)
                    });
                    if both_bool {
                        return false;
                    }
                    // A `T & FLAG` / `T | FLAG` whose operand type defines a
                    // bool-returning `impl BitAnd`/`BitOr` (a bitflags newtype)
                    // yields `bool` only through that custom operator; the
                    // operands are `T`, not `bool`, so `&&`/`||` would not
                    // type-check and cannot be the intent — same reasoning as
                    // the `^` exemption.
                    return !bitop_operand_type_has_bool_output_impl(node, source);
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if has_bitwise_op(child, source) {
                    return true;
                }
            }
            false
        }
        "parenthesized_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if has_bitwise_op(child, source) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

crate::ast_check! { on ["if_expression", "while_expression"] => |node, source, ctx, diagnostics|
    let Some(condition) = node.child_by_field_name("condition") else { return };

    if has_bitwise_op(condition, source) {
        let pos = condition.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-bitwise-in-boolean".into(),
            message: "Bitwise operator in boolean context — did you mean `&&`/`||`?".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
    fn flags_bitwise_on_unprovable_operands() {
        // Operands whose bool-ness can't be proven from the AST (struct fields):
        // still ambiguous between a `&&` typo and the branchless idiom, so fire.
        assert_eq!(run_on("fn f(s: &S) { if s.a & s.b {} }").len(), 1);
        assert_eq!(run_on("fn f(s: &S) { if s.foo() | s.bar() {} }").len(), 1);
    }

    #[test]
    fn allows_logical_and() {
        assert!(run_on("fn f(a: bool, b: bool) { if a && b {} }").is_empty());
    }

    #[test]
    fn allows_bitmask_test() {
        assert!(run_on("fn f(state: u32) { if state & FLAG == 0 {} }").is_empty());
        assert!(run_on("fn f(state: u32) { while state & MASK != 0 {} }").is_empty());
    }

    #[test]
    fn allows_branchless_op_on_bool_bindings() {
        // Both operands are `bool`-typed locals: `bool & bool` / `bool | bool`
        // type-checks only on bools, so this is the branchless logical idiom,
        // not a `&&`/`||` typo (#5629).
        assert!(run_on("fn f(a: bool, b: bool) { if a & b {} }").is_empty());
        assert!(run_on("fn f(a: bool, b: bool) { if a | b {} }").is_empty());
    }

    #[test]
    fn allows_branchless_or_of_comparisons() {
        // revm hot-path `dup`: `bool | bool` is the non-short-circuit logical
        // OR idiom, both operands provably boolean — not a `||` typo (#5629).
        assert!(
            run_on(
                "fn f(len: usize, n: usize) { if (len < n) | (len + 1 > STACK_LIMIT) { return; } }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_branchless_and_of_comparisons() {
        assert!(
            run_on("fn f(a: u32, b: u32, c: u32, d: u32) { let ok = (a == b) & (c != d); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_branchless_ops_with_negation_and_literal() {
        assert!(run_on("fn f(a: bool, b: u32) { if !a | (b > 0) {} }").is_empty());
        assert!(run_on("fn f(a: u32) { if (a > 0) & true {} }").is_empty());
    }

    #[test]
    fn still_flags_integer_operand() {
        // One operand is a plain integer-typed expression: intent is ambiguous,
        // keep flagging.
        assert_eq!(run_on("fn f(mask: u32) { if (mask > 0) | mask {} }").len(), 1);
    }

    #[test]
    fn allows_xor_as_logical_in_if() {
        // `bool ^ bool` is the only way to express logical XOR in Rust; `^`
        // consumed directly as a boolean condition is always intentional.
        assert!(run_on("fn f(a: bool, b: bool) { if a ^ b {} }").is_empty());
        assert!(
            run_on("fn f(flag: bool) { let x: Result<(), ()> = Ok(()); if x.is_ok() ^ flag {} }")
                .is_empty()
        );
    }

    #[test]
    fn allows_xor_under_logical_operators() {
        // `^` nested under `&&`/`||`/`!` is still consumed as a boolean.
        assert!(run_on("fn f(a: bool, b: bool, c: bool) { if (a ^ b) && c {} }").is_empty());
        assert!(run_on("fn f(a: bool, b: bool) { if !(a ^ b) {} }").is_empty());
    }

    #[test]
    fn still_flags_xor_feeding_comparison() {
        // `(a ^ b) == 0` is integer XOR feeding a comparison: already out of
        // scope for this boolean-typo rule.
        assert!(run_on("fn f(a: u32, b: u32) { if a ^ b == 0 {} }").is_empty());
    }

    #[test]
    fn still_flags_and_or_nested_under_xor() {
        // `^` is exempt, but a `&`/`|` on unprovable operands nested inside it
        // must still fire.
        assert_eq!(run_on("fn f(s: &S, c: bool) { if (s.a & s.b) ^ c {} }").len(), 1);
    }

    #[test]
    fn allows_custom_bitand_with_bool_output() {
        // `TokModes` is a bitflags newtype whose `impl BitAnd { type Output = bool }`
        // makes `mode & FLAG` a `bool`; `mode && FLAG` would be a type error, so
        // the `&` is not a short-circuit typo (#7284).
        let src = "struct TokModes(u8); \
            impl BitAnd for TokModes { type Output = bool; fn bitand(self, rhs: Self) -> bool { (self.0 & rhs.0) != 0 } } \
            const A: TokModes = TokModes(1); \
            fn f(mode: TokModes) { if mode & A { } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_custom_bitand_when_flag_operand_is_typed_const() {
        // Mirrors fish tokenizer.rs: the local `mode` is inferred (no annotation),
        // but the flag operand is a `const: TokModes`, so the operand type resolves
        // through the const and the bool-returning `BitAnd` impl exempts the `&`.
        let src = "struct TokModes(u8); \
            impl BitAnd for TokModes { type Output = bool; fn bitand(self, rhs: Self) -> bool { (self.0 & rhs.0) != 0 } } \
            const TOK_MODE_REGULAR_TEXT: TokModes = TokModes(0); \
            const TOK_MODE_CHAR_ESCAPE: TokModes = TokModes(8); \
            fn scan() { let mut mode = TOK_MODE_REGULAR_TEXT; if mode & TOK_MODE_CHAR_ESCAPE { mode = TOK_MODE_REGULAR_TEXT; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_typed_operand_without_bool_output_impl() {
        // The operand type resolves, but there is no `impl BitAnd … { type Output =
        // bool }` for it: `mode & A` stays ambiguous (a genuine `&&` typo cannot be
        // ruled out), so it fires. Suppression is gated on the impl, not on merely
        // resolving a type.
        let src = "struct TokModes(u8); \
            const A: TokModes = TokModes(1); \
            fn f(mode: TokModes) { if mode & A { } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_when_bool_output_impl_is_for_a_different_type() {
        // A bool-returning `BitAnd` impl exists in the file, but for an unrelated
        // type; the operand's type has no such impl, so the `&` still fires. Guards
        // the Self-type match — suppression is not "any bitflags impl in the file".
        let src = "struct TokModes(u8); struct OtherFlags(u8); \
            impl BitAnd for OtherFlags { type Output = bool; fn bitand(self, rhs: Self) -> bool { (self.0 & rhs.0) != 0 } } \
            const A: TokModes = TokModes(1); \
            fn f(mode: TokModes) { if mode & A { } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_when_bitand_output_is_not_bool() {
        // The custom `BitAnd` yields the newtype itself, not `bool`, so the `&` is
        // not a boolean-condition operator; keep flagging.
        let src = "struct TokModes(u8); \
            impl BitAnd for TokModes { type Output = TokModes; fn bitand(self, rhs: Self) -> TokModes { TokModes(self.0 & rhs.0) } } \
            const A: TokModes = TokModes(1); \
            fn f(mode: TokModes) { if mode & A { } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
