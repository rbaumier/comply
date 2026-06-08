//! prefer-math-trunc — flag bitwise truncation patterns that should be
//! `Math.trunc(x)`.
//!
//! Detection:
//!   - `unary_expression` whose operator is `~` and whose operand is
//!     itself `~expr` → `~~x`.
//!   - `binary_expression` with operator in `|`, `>>`, `<<`, `^` whose
//!     right operand is the numeric literal `0` → `x | 0`, `x >> 0`, etc.
//!   - `augmented_assignment_expression` with operator in `|=`, `>>=`,
//!     `<<=`, `^=` whose right operand is the numeric literal `0`.

use crate::diagnostic::{Diagnostic, Severity};

const BITWISE_TRUNC_OPS: &[&str] = &["|", ">>", "<<", "^"];
const BITWISE_TRUNC_ASSIGN_OPS: &[&str] = &["|=", ">>=", "<<=", "^="];

fn is_zero_literal(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "number" && node.utf8_text(source).unwrap_or("") == "0"
}

crate::ast_check! { on ["unary_expression", "binary_expression", "augmented_assignment_expression"] prefilter = ["~~"] => |node, source, ctx, diagnostics|
match node.kind() {
        "unary_expression" => {
            // ~~x — outer unary is `~`, argument is another `~expr`.
            let Some(op) = node.child_by_field_name("operator") else { return };
            if op.utf8_text(source).unwrap_or("") != "~" {
                return;
            }
            let Some(arg) = node.child_by_field_name("argument") else { return };
            if arg.kind() != "unary_expression" {
                return;
            }
            let Some(inner_op) = arg.child_by_field_name("operator") else { return };
            if inner_op.utf8_text(source).unwrap_or("") != "~" {
                return;
            }
            // Don't double-flag the inner unary when the walker visits it.
            // We only fire on the outer `~~`. Skip if our parent is also `~`
            // (i.e. `~~~x` — the outer `~~` is the parent's argument).
            if let Some(parent) = node.parent()
                && parent.kind() == "unary_expression"
                && parent.child_by_field_name("operator")
                    .and_then(|n| n.utf8_text(source).ok()) == Some("~")
            {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "prefer-math-trunc",
                "Use `Math.trunc(x)` instead of `~~x`.".into(),
                Severity::Warning,
            ));
        }
        "binary_expression" => {
            let Some(op_node) = node.child_by_field_name("operator") else { return };
            let Some(op) = op_node.utf8_text(source).ok() else { return };
            if !BITWISE_TRUNC_OPS.contains(&op) {
                return;
            }
            let Some(right) = node.child_by_field_name("right") else { return };
            if !is_zero_literal(right, source) {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "prefer-math-trunc",
                format!("Use `Math.trunc(x)` instead of bitwise `{op} 0`."),
                Severity::Warning,
            ));
        }
        "augmented_assignment_expression" => {
            let Some(op_node) = node.child_by_field_name("operator") else { return };
            let Some(op) = op_node.utf8_text(source).ok() else { return };
            if !BITWISE_TRUNC_ASSIGN_OPS.contains(&op) {
                return;
            }
            let Some(right) = node.child_by_field_name("right") else { return };
            if !is_zero_literal(right, source) {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "prefer-math-trunc",
                format!("Use `Math.trunc(x)` instead of bitwise assignment `{op} 0`."),
                Severity::Warning,
            ));
        }
        _ => {}
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bitwise_or_zero() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const n = value | 0;", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-math-trunc");
    }

    #[test]
    fn flags_double_tilde() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const n = ~~value;", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_math_trunc() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const n = Math.trunc(value);", "t.ts").is_empty());
    }

    #[test]
    fn ignores_string_literal() {
        assert!(crate::rules::test_helpers::run_rule(&Check, r#"const s = "value | 0";"#, "t.ts").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "// value | 0", "t.ts").is_empty());
    }
}
