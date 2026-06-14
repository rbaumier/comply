//! no-bitwise-in-boolean Rust backend.
//!
//! Flag bitwise ops (`&`, `|`) in boolean contexts (if/while conditions). `^` is
//! not flagged: it is the only way to express logical XOR on `bool` in Rust.

use crate::diagnostic::{Diagnostic, Severity};

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
                    return true;
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
    fn flags_bitwise_in_if() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if a & b {} }").len(), 1);
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
    fn flags_bare_bitwise_without_comparison() {
        assert_eq!(run_on("fn f(a: bool, b: bool) { if a | b {} }").len(), 1);
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
        // `^` is exempt, but a `&`/`|` typo nested inside it must still fire.
        assert_eq!(run_on("fn f(a: bool, b: bool, c: bool) { if (a & b) ^ c {} }").len(), 1);
    }
}
