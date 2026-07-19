//! explicit-length-check — OXC backend.
//! Flags a bare `.length`/`.size` member access that is coerced to boolean
//! (`if (arr.length)`, `!arr.length`) so the author writes an explicit numeric
//! comparison instead. The coercion-vs-value distinction is read from the AST
//! parent chain of the member access, so it holds regardless of how the base
//! expression wraps across source lines.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when the `.length`/`.size` member access at `node` sits in a boolean
/// coercion position — its value is implicitly tested for truthiness rather than
/// consumed as a number — determined by walking up the AST parent chain.
///
/// Coercion positions (flagged): the test of an `if`/`while`/`do-while`/`for`,
/// the test of a conditional expression, the operand of logical-NOT `!`, or an
/// operand that itself reaches such a position through `&&`/`||`, parentheses, or
/// an optional chain. Every other parent — a variable initializer, assignment
/// right-hand side, call/`new` argument, arithmetic/comparison operand,
/// `return`/property value, template interpolation, or `??` operand — is a value
/// position and is not flagged. `&&`/`||` operands coerce only when the logical
/// expression's own result lands in a boolean context (`if (a.length && b)`), so
/// the walk continues upward through them.
fn is_boolean_coercion_position(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{LogicalOperator, UnaryOperator};

    let nodes = semantic.nodes();
    let mut child_span = node.kind().span();
    let mut cur = node.id();
    loop {
        let parent_id = nodes.parent_id(cur);
        if parent_id == cur {
            // Reached the program root with no boolean-test ancestor.
            return false;
        }
        match nodes.kind(parent_id) {
            AstKind::IfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            AstKind::ForStatement(for_stmt) => {
                // Only the `for (…; test; …)` condition slot is a coercion; the
                // init/update slots are value positions.
                return for_stmt
                    .test
                    .as_ref()
                    .is_some_and(|test| test.span() == child_span);
            }
            AstKind::UnaryExpression(unary) => {
                return unary.operator == UnaryOperator::LogicalNot;
            }
            AstKind::ConditionalExpression(cond) => {
                if cond.test.span() == child_span {
                    return true;
                }
                // A consequent/alternate branch is a value the ternary yields; its
                // context is the ternary's own, so keep walking up.
            }
            AstKind::LogicalExpression(logical) => {
                // `??` selects a value, never a boolean coercion.
                if logical.operator == LogicalOperator::Coalesce {
                    return false;
                }
            }
            AstKind::ParenthesizedExpression(_) | AstKind::ChainExpression(_) => {
                // Transparent wrappers — the wrapped value keeps its position.
            }
            _ => return false,
        }
        child_span = nodes.kind(parent_id).span();
        cur = parent_id;
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".length", ".size"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop != "length" && prop != "size" {
            return;
        }
        if !is_boolean_coercion_position(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use explicit length comparison: `arr.length > 0` instead of \
                      `arr.length`, or `arr.length === 0` instead of `!arr.length`."
                .into(),
            severity: Severity::Error,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_length_in_if() {
        assert_eq!(run_on("if (arr.length) {}").len(), 1);
    }

    #[test]
    fn flags_negated_length_in_if() {
        assert_eq!(run_on("if (!arr.length) {}").len(), 1);
    }

    #[test]
    fn allows_explicit_greater_than_zero() {
        assert!(run_on("if (arr.length > 0) {}").is_empty());
    }

    #[test]
    fn allows_assignment() {
        assert!(run_on("const len = arr.length;").is_empty());
    }

    #[test]
    fn allows_expect_to_be_greater_than() {
        assert!(run_on("expect(value.length).toBeGreaterThan(0);").is_empty());
    }

    #[test]
    fn allows_expect_to_be() {
        assert!(run_on("expect(arr.length).toBe(3);").is_empty());
    }

    #[test]
    fn allows_expect_to_equal() {
        assert!(run_on("expect(arr.length).toEqual(0);").is_empty());
    }

    #[test]
    fn allows_length_inside_nested_call_inside_expect() {
        // Real-world pattern: filter then check length non-empty.
        let src = "expect(arr.filter(x => x.foo).length).toBeGreaterThan(0);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_length_inside_object_keys_inside_expect() {
        let src = "expect(Object.keys(obj).length).toBe(3);";
        assert!(run_on(src).is_empty());
    }

    // Regression #6247 — `.length` as the sole argument of any function call is a
    // numeric value position, not the implicit boolean coercion the rule targets.
    #[test]
    fn allows_length_as_sole_call_argument_issue_6247() {
        assert!(run_on("notExpect(arr.length);").is_empty());
    }

    // Regression for #259: `.length` read as an object-property value is a
    // numeric use, not a boolean coercion.
    #[test]
    fn allows_length_as_object_property_value() {
        assert!(run_on("count: list.length,").is_empty());
    }

    // Regression #589 — `.length` as a numeric `slice` argument, not boolean.
    #[test]
    fn allows_length_as_slice_argument_issue_589() {
        assert!(run_on("const head = full.slice(0, prefix.length);").is_empty());
    }

    // Regression #589 — comparing two lengths is already an explicit check.
    #[test]
    fn allows_two_length_comparison_issue_589() {
        assert!(run_on("if (found.length !== uniqueTeamIds.length) {}").is_empty());
    }

    #[test]
    fn allows_two_length_comparison_in_return_issue_589() {
        assert!(run_on("return found.length === expected.length;").is_empty());
    }

    // Regression #6247 — `.length` handed to `Boolean(...)` is a call argument
    // (value position); the explicit coercion is `Boolean`'s, so the bare
    // `.length` is not the implicit truthy check the rule targets.
    #[test]
    fn allows_length_as_boolean_call_argument_issue_6247() {
        assert!(run_on("if (Boolean(arr.length)) {}").is_empty());
    }

    // Regression #3914 — a ternary branch is a numeric value position, not a
    // boolean coercion. The consequent / alternate can sit on its own line.
    #[test]
    fn allows_length_as_ternary_consequent_issue_3914() {
        assert!(run_on("const n = cond ? obj.length : 0;").is_empty());
    }

    #[test]
    fn allows_length_as_ternary_alternate_issue_3914() {
        assert!(run_on("const n = cond ? 0 : obj.length;").is_empty());
    }

    #[test]
    fn allows_length_as_split_ternary_consequent_issue_3914() {
        // prettier src/language-yaml/utilities.js:221 shape.
        let src = "x = matches\n  ? matches.groups.leadingSpace.length\n  : Number.POSITIVE_INFINITY;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_length_as_split_ternary_alternate_issue_3914() {
        let src = "x = cond\n  ? 0\n  : obj.length;";
        assert!(run_on(src).is_empty());
    }

    // `:` also marks an object-property value — a value position, not coercion.
    #[test]
    fn allows_length_as_object_property_value_brace_issue_3914() {
        assert!(run_on("{ k: arr.length }").is_empty());
    }

    // Optional chaining in a boolean test MUST STILL FLAG: the optional-chain
    // wrapper is transparent, so `obj?.b.length` in an `if` test is still a
    // coercion, while as a call argument it stays a value position.
    #[test]
    fn still_flags_optional_chain_length_in_if_issue_3914() {
        assert_eq!(run_on("if (a?.b.length) {}").len(), 1);
    }

    #[test]
    fn allows_optional_chain_length_as_call_arg_issue_3914() {
        assert_eq!(run_on("notExpect(a?.b.length);").len(), 0);
    }

    // Regression #3785 — `.length`/`.size` as the value of a template-literal
    // interpolation `${...}` is a numeric value being string-formatted, not a
    // boolean coercion.
    #[test]
    fn allows_length_as_template_interpolation_value_issue_3785() {
        assert!(run_on("console.log(`count: ${items.length}`);").is_empty());
    }

    #[test]
    fn allows_length_as_template_interpolation_value_with_whitespace_issue_3785() {
        assert!(run_on("`${ items.length }`").is_empty());
    }

    #[test]
    fn allows_size_as_template_interpolation_value_issue_3785() {
        assert!(run_on("`set: ${mySet.size}`").is_empty());
    }

    #[test]
    fn allows_member_base_length_as_template_interpolation_value_issue_3785() {
        assert!(run_on("`${obj.items.length}`").is_empty());
    }

    // A ternary condition inside an interpolation IS a genuine boolean coercion
    // (`.length` is the ternary test) and must still flag.
    #[test]
    fn still_flags_length_as_ternary_condition_in_interpolation_issue_3785() {
        assert_eq!(run_on("`${arr.length ? 'a' : 'b'}`").len(), 1);
    }

    // Regression #3788 — `.length`/`.size` as an operand of a binary arithmetic
    // expression is a numeric operand, not a boolean coercion.
    #[test]
    fn allows_length_after_addition_issue_3788() {
        assert!(run_on("const buf = Buffer.allocUnsafe(30 + nameBytes.length);").is_empty());
    }

    #[test]
    fn allows_length_after_modulo_issue_3788() {
        assert!(run_on("next = (next + 1) % options.length;").is_empty());
    }

    #[test]
    fn allows_length_after_division_issue_3788() {
        assert!(run_on("const avg = total / sizes.length;").is_empty());
    }

    #[test]
    fn allows_length_after_multiplication_issue_3788() {
        assert!(run_on("const x = a * arr.length;").is_empty());
    }

    #[test]
    fn allows_length_after_subtraction_issue_3788() {
        assert!(run_on("const y = a - arr.length;").is_empty());
    }

    #[test]
    fn allows_size_after_addition_issue_3788() {
        assert!(run_on("const z = 5 + mySet.size;").is_empty());
    }

    // Regression #6247 — `.length` as the sole/first argument of a call is a
    // numeric index, not a boolean coercion. The markedjs/marked repro plus the
    // generic `foo(a.length)` / `indexOf(s.length)` shapes.
    #[test]
    fn allows_length_as_first_call_argument_issue_6247() {
        assert!(run_on("src = src.substring(token.raw.length);").is_empty());
    }

    #[test]
    fn allows_length_as_plain_call_argument_issue_6247() {
        assert!(run_on("foo(a.length);").is_empty());
    }

    #[test]
    fn allows_length_as_index_of_argument_issue_6247() {
        assert!(run_on("indexOf(s.length);").is_empty());
    }

    // Soundness guard for #6247: a grouping/test paren is NOT a value position, so
    // a `.length` parenthesised inside a boolean test still flags.
    #[test]
    fn still_flags_parenthesised_length_in_if_issue_6247() {
        assert_eq!(run_on("if ((arr.length)) {}").len(), 1);
    }

    #[test]
    fn still_flags_parenthesised_length_in_while_issue_6247() {
        assert_eq!(run_on("while ((list.length)) {}").len(), 1);
    }

    #[test]
    fn still_flags_parenthesised_length_as_ternary_condition_issue_6247() {
        assert_eq!(run_on("const x = (items.length) ? a : b;").len(), 1);
    }

    // Regression #6475 — `.length` used as a computed-member index
    // (`expr[base.length]`) is a positional numeric index, not a boolean
    // coercion. The unjs/ufo repro plus the generic `arr[other.length]` shape.
    #[test]
    fn allows_length_as_computed_member_index_issue_6475() {
        assert!(run_on("const nextChar = input[_base.length];").is_empty());
    }

    #[test]
    fn allows_length_as_computed_member_index_generic_issue_6475() {
        assert!(run_on("arr[other.length]").is_empty());
    }

    // The array-literal-element form (`[base.length]`, base at the start of the
    // brackets) is the same value position as a computed index.
    #[test]
    fn allows_length_as_array_literal_element_issue_6475() {
        assert!(run_on("const dims = [rows.length];").is_empty());
    }

    // Soundness guard for #6475: a computed index inside the base
    // (`arr[i].length`) is walked past, so a genuine boolean coercion of
    // `arr[i].length` in a test position still flags.
    #[test]
    fn still_flags_indexed_base_length_in_if_issue_6475() {
        assert_eq!(run_on("if (arr[i].length) {}").len(), 1);
    }

    // Regression #7202 — `.length` whose base call spans multiple lines is still a
    // variable-initializer value position: the `const … =` establishing the value
    // context sits two physical lines above the `).length;` line, out of a
    // line-local scanner's reach, but the AST parent (a `VariableDeclarator`) is
    // unambiguous.
    #[test]
    fn allows_length_on_multiline_filter_assignment_issue_7202() {
        let src = "const auditCount = this.audits.filter(\n\
                   \t(audit) => getAuditCategory(audit.rule) === category.code,\n\
                   ).length;";
        assert!(run_on(src).is_empty());
    }

    // Control for #7202: the same value-position classification holds for a bare
    // `return` and for `.length` as the left operand of arithmetic.
    #[test]
    fn allows_bare_length_in_return_issue_7202() {
        assert!(run_on("return arr.length;").is_empty());
    }

    #[test]
    fn allows_length_plus_one_issue_7202() {
        assert!(run_on("const x = arr.length + 1;").is_empty());
    }

    // Control for #7202: a bare `.length` in a `while` test is still a coercion
    // and must still flag — the AST migration must not lose true positives.
    #[test]
    fn still_flags_bare_length_in_while_issue_7202() {
        assert_eq!(run_on("while (arr.length) {}").len(), 1);
    }
}
