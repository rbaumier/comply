//! prefer-code-point oxc backend — flag `charCodeAt` and `String.fromCharCode`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when the value of `node` is fed directly into an arithmetic or bitwise
/// computation — operand of a binary/unary arithmetic-or-bitwise operator or a
/// compound arithmetic/bitwise assignment, looking through parentheses.
///
/// `charCodeAt()` returns a guaranteed `number`; `codePointAt()` returns
/// `number | undefined`. In an arithmetic/bitwise context the suggested
/// `codePointAt()` would inject `| undefined` (a strict-TS error or runtime
/// NaN), and the astral-character handling that motivates the rule is
/// irrelevant — such code decodes single-byte/protocol values, not text. Only
/// genuine text processing (string round-trips, comparisons) keeps the
/// suggestion.
fn is_arithmetic_or_bitwise_consumer<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current = node.id();
    loop {
        let parent = semantic.nodes().parent_node(current);
        match parent.kind() {
            // `(s.charCodeAt(i)) - 32` — parens are preserved in the AST.
            AstKind::ParenthesizedExpression(_) => {
                current = parent.id();
            }
            AstKind::BinaryExpression(bin) => {
                return bin.operator.is_arithmetic() || bin.operator.is_bitwise();
            }
            AstKind::UnaryExpression(unary) => {
                return unary.operator.is_arithmetic() || unary.operator.is_bitwise();
            }
            AstKind::AssignmentExpression(assign) => {
                return assign.operator.is_arithmetic() || assign.operator.is_bitwise();
            }
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["charCodeAt", "fromCharCode"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let prop_name = member.property.name.as_str();
        match prop_name {
            "charCodeAt" => {
                if is_arithmetic_or_bitwise_consumer(node, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `String#codePointAt()` over `String#charCodeAt()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            "fromCharCode" => {
                // Verify object is `String`
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if obj.name.as_str() != "String" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `String.fromCodePoint()` over `String.fromCharCode()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // ---- #5725 regressions: arithmetic/bitwise consumption -> must NOT flag ----

    #[test]
    fn ignores_subtraction_arithmetic() {
        // MouseTracking.test.ts:144 — X10 mouse tracking byte decode.
        let code = "const col = report.charCodeAt(4) - 32;";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_bitwise_and() {
        let code = "const byte = s.charCodeAt(i) & 0xff;";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_unary_negation() {
        let code = "const n = -s.charCodeAt(i);";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_bitwise_not() {
        let code = "const n = ~s.charCodeAt(i);";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_shift() {
        let code = "const n = s.charCodeAt(i) << 8;";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_compound_bitwise_assignment() {
        let code = "let acc = 0; acc |= s.charCodeAt(i);";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_arithmetic_through_parens() {
        let code = "const n = (s.charCodeAt(i)) - 0x30;";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    // ---- genuine text processing -> MUST still flag ----

    #[test]
    fn flags_round_trip_through_from_char_code() {
        // String.fromCharCode(s.charCodeAt(i)) — astral handling matters here.
        let code = "const c = String.fromCharCode(s.charCodeAt(i));";
        // Flags both the charCodeAt argument and String.fromCharCode itself.
        assert_eq!(run(code).len(), 2, "{:?}", run(code));
    }

    #[test]
    fn flags_bare_char_code_at() {
        let code = "const code = s.charCodeAt(0);";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_equality_comparison() {
        // Comparison is text logic, not arithmetic — keep the suggestion.
        let code = "if (s.charCodeAt(0) === 0x41) {}";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_relational_comparison() {
        // Range check is text logic, not arithmetic — keep the suggestion.
        let code = "if (s.charCodeAt(0) < 0x80) {}";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_typed_array_slot_assignment() {
        // Plain `=` into a TypedArray is not arithmetic consumption.
        let code = "result[i] = s.charCodeAt(i);";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_from_char_code() {
        let code = "const s = String.fromCharCode(65);";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }
}
