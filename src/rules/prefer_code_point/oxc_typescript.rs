//! prefer-code-point oxc backend — flag `charCodeAt` and `String.fromCharCode`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentOperator, AssignmentTarget, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when `call` invokes `String.fromCharCode(...)` — the text round-trip
/// whose `charCodeAt()` argument should still be flagged so both APIs are
/// upgraded together to their code-point equivalents.
fn is_string_from_char_code(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "fromCharCode" {
        return false;
    }
    matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "String")
}

/// How a value is consumed by its nearest non-parenthesis parent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Consumption {
    /// Operand of an arithmetic/bitwise operator or compound assignment, a
    /// direct call argument (except `String.fromCharCode`), or a computed index
    /// — positions where the code unit is a plain integer and `codePointAt()`'s
    /// `number | undefined` (or astral pairing) would break, while the
    /// astral-character concern the rule targets is irrelevant.
    Arithmetic,
    /// Operand of an equality or relational comparison — a code-unit value or
    /// range check, not text processing, so it neither needs `codePointAt()` nor
    /// forces the suggestion.
    Comparison,
    /// Any other position — a string round-trip via `String.fromCharCode`, a
    /// plain store, a property access — genuine text handling where the
    /// `codePointAt()` suggestion stands.
    Other,
}

/// Classifies how the value produced at `node` is consumed by inspecting its
/// nearest non-parenthesis parent. Does not follow the value through a binding.
fn classify_direct_consumption<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Consumption {
    let mut current = node.id();
    loop {
        let parent = semantic.nodes().parent_node(current);
        match parent.kind() {
            // `(s.charCodeAt(i)) - 32` — parens are preserved in the AST.
            AstKind::ParenthesizedExpression(_) => {
                current = parent.id();
            }
            AstKind::BinaryExpression(bin) => {
                return if bin.operator.is_arithmetic() || bin.operator.is_bitwise() {
                    Consumption::Arithmetic
                } else if bin.operator.is_equality() || bin.operator.is_compare() {
                    Consumption::Comparison
                } else {
                    Consumption::Other
                };
            }
            AstKind::UnaryExpression(unary) => {
                return if unary.operator.is_arithmetic() || unary.operator.is_bitwise() {
                    Consumption::Arithmetic
                } else {
                    Consumption::Other
                };
            }
            AstKind::AssignmentExpression(assign) => {
                return if assign.operator.is_arithmetic() || assign.operator.is_bitwise() {
                    Consumption::Arithmetic
                } else {
                    Consumption::Other
                };
            }
            // `fn(s.charCodeAt(i))` — passed directly as a call argument. The
            // return type is a guaranteed `number`; `codePointAt()` returns
            // `number | undefined`, which would break a callee whose parameter
            // is typed `number` (a strict-TS error). The exception is
            // `String.fromCharCode(...)`, the text round-trip that must still
            // flag so both APIs are upgraded together.
            AstKind::CallExpression(parent_call) => {
                let current_span = semantic.nodes().get_node(current).kind().span();
                let is_argument = parent_call
                    .arguments
                    .iter()
                    .any(|arg| arg.span() == current_span);
                return if is_argument && !is_string_from_char_code(parent_call) {
                    Consumption::Arithmetic
                } else {
                    Consumption::Other
                };
            }
            // `array[s.charCodeAt(i)]` — used as the computed array index. The
            // code unit is consumed as a plain integer key, identical to
            // arithmetic; `codePointAt()` would return `number | undefined` and
            // `array[undefined]` silently changes runtime behavior without
            // fixing any Unicode issue. Exempt only the index position, not the
            // object (`s.charCodeAt(i)[0]`, where the call is indexed into).
            AstKind::ComputedMemberExpression(member) => {
                let current_span = semantic.nodes().get_node(current).kind().span();
                return if member.expression.span() == current_span {
                    Consumption::Arithmetic
                } else {
                    Consumption::Other
                };
            }
            _ => return Consumption::Other,
        }
    }
}

/// True when every read of the local bound to `symbol` consumes the value only
/// arithmetically (with equality/relational comparisons tolerated) and at least
/// one such arithmetic consumer exists. This is the single binding hop: a code
/// unit assigned to a loop-local and folded into an accumulator — the canonical
/// hand-rolled string-hash shape (`const c = s.charCodeAt(i); h = (h << 5) + c`)
/// — never reaches text handling, so `codePointAt()` would be wrong. A single
/// read reaching genuine text processing (`Other`) re-enables the diagnostic;
/// write references (the assignment/reassignment targets) are not reads.
fn binding_reads_only_arithmetic<'a>(
    symbol: oxc_semantic::SymbolId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_semantic::ReferenceFlags;

    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut has_arithmetic_consumer = false;
    for reference in scoping.get_resolved_references(symbol) {
        if !reference.flags().contains(ReferenceFlags::Read) {
            continue;
        }
        let ref_node = nodes.get_node(reference.node_id());
        match classify_direct_consumption(ref_node, semantic) {
            Consumption::Arithmetic => has_arithmetic_consumer = true,
            Consumption::Comparison => {}
            Consumption::Other => return false,
        }
    }
    has_arithmetic_consumer
}

/// True when the `charCodeAt()` call at `node` yields a value consumed only in
/// arithmetic/bitwise/call-argument/index positions, so the `codePointAt()`
/// suggestion would inject `number | undefined` (or mis-handle an astral pair)
/// and must be suppressed.
///
/// The value is followed one hop through a local binding: when the call is the
/// initializer of a `const c = s.charCodeAt(i)` declarator or the right side of
/// a plain `c = s.charCodeAt(i)` assignment into a simple identifier, the
/// binding is resolved and the same consumer test is applied to every read of
/// it. Consumption without a binding is classified in place.
fn is_arithmetic_or_bitwise_consumer<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current = node.id();
    loop {
        let parent = semantic.nodes().parent_node(current);
        match parent.kind() {
            // `(s.charCodeAt(i))` — look through parens to the real consumer.
            AstKind::ParenthesizedExpression(_) => {
                current = parent.id();
            }
            // `const c = s.charCodeAt(i)` — resolve the binding and apply the
            // consumer test to its reads.
            AstKind::VariableDeclarator(decl) => {
                return decl
                    .id
                    .get_binding_identifier()
                    .and_then(|binding| binding.symbol_id.get())
                    .is_some_and(|symbol| binding_reads_only_arithmetic(symbol, semantic));
            }
            // `c = s.charCodeAt(i)` (plain `=`) into a simple identifier — same
            // one-hop resolution. Compound assignments (`c |= …`) and non-plain
            // targets (`arr[i] = …`) fall through to in-place classification.
            AstKind::AssignmentExpression(assign)
                if matches!(assign.operator, AssignmentOperator::Assign) =>
            {
                let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
                    return false;
                };
                return semantic
                    .scoping()
                    .get_reference(target.reference_id())
                    .symbol_id()
                    .is_some_and(|symbol| binding_reads_only_arithmetic(symbol, semantic));
            }
            _ => return classify_direct_consumption(node, semantic) == Consumption::Arithmetic,
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

    #[test]
    fn ignores_direct_function_argument() {
        // noble-hashes src/utils.ts:554 — asciiToBase16(ch: number) requires
        // `number`; codePointAt() would inject `number | undefined`.
        let code = "const n1 = asciiToBase16(hex.charCodeAt(hi));";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_function_argument_with_offset_index() {
        let code = "const n2 = asciiToBase16(hex.charCodeAt(hi + 1));";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_computed_array_index() {
        // undici lib/core/util.js — isValidHTTPToken: charCodeAt() keys a lookup
        // table; the code unit is consumed as a plain integer index.
        let code = "if (validTokenChars[characters.charCodeAt(i)] !== 1) {}";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn flags_char_code_at_indexed_into() {
        // `s.charCodeAt(i)[0]` — the call is the indexed object, not the index;
        // the span guard must not exempt it.
        let code = "const x = s.charCodeAt(i)[0];";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
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

    // ---- #7730 regressions: code unit bound to a local, then folded into an
    // accumulator (hand-rolled string hashes) -> must NOT flag ----

    #[test]
    fn ignores_binding_consumed_by_arithmetic() {
        // next.js hash.ts:11 (djb2) — `char` folded via `+` and `&`.
        let code = "const char = s.charCodeAt(i); hash = ((hash << 5) + hash + char) & 0xffffffff;";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_binding_consumed_by_bitwise_and_call() {
        // next.js bloom-filter.ts:5 (murmurhash2) — `c` used via `^` and Math.imul.
        let code = "const c = s.charCodeAt(i); h = Math.imul(h ^ c, 0x5bd1e995);";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn ignores_binding_compared_then_folded_including_reassignment() {
        // next.js fnv1a.ts:58 & :63 — a `let` compared (`> 0x7f`), reassigned by
        // a second charCodeAt, then folded via BigInt(...) and `^=`. Both the
        // declarator and the plain-`=` reassignment site must be suppressed.
        let code =
            "let ch = s.charCodeAt(i); if (ch > 0x7f) { ch = s.charCodeAt(i); } hash ^= BigInt(ch);";
        assert!(run(code).is_empty(), "{:?}", run(code));
    }

    #[test]
    fn flags_binding_round_tripped_to_text() {
        // The bound value reaches `String.fromCharCode` — genuine text
        // processing that `codePointAt()` is meant to fix; keep the suggestion.
        let code = "const ch = s.charCodeAt(i); result += String.fromCharCode(ch);";
        // Flags both the charCodeAt binding and String.fromCharCode itself.
        assert_eq!(run(code).len(), 2, "{:?}", run(code));
    }

    #[test]
    fn flags_binding_compared_equality_only() {
        // A local read only by an equality comparison is text logic, not an
        // arithmetic consumer — mirror the direct-parent policy and keep the
        // suggestion (the tolerated `Comparison` read must not suppress alone).
        let code = "const ch = s.charCodeAt(i); if (ch === 0x41) {}";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_binding_compared_relational_only() {
        let code = "const ch = s.charCodeAt(i); if (ch < 0x80) {}";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }
}
