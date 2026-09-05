//! expression-complexity oxc backend — flag lines with 4+ logical/conditional operators.
//!
//! The threshold is counted per source line, so one crowded line yields one
//! diagnostic however many expressions it holds. The diagnostic is anchored on
//! the first operator expression of that line, not on the left margin.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, UnaryOperator};
use oxc_span::Span;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Reports whether every operand of the logical chain is a bare identifier.
///
/// The walk descends the logical spine plus the two transparent wrappers a
/// boolean operand can carry — parentheses and `!`. Every other expression
/// kind is an operand that still holds an unnamed expression: a call, a
/// member access, a comparison, `typeof x`.
fn every_operand_is_named(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_) => true,
        Expression::ParenthesizedExpression(paren) => every_operand_is_named(&paren.expression),
        Expression::UnaryExpression(un) => {
            un.operator == UnaryOperator::LogicalNot && every_operand_is_named(&un.argument)
        }
        Expression::LogicalExpression(log) => {
            every_operand_is_named(&log.left) && every_operand_is_named(&log.right)
        }
        _ => false,
    }
}

/// Reports whether the chain `node_id` belongs to already names every one of
/// its operands — the remediation has been applied and the diagnostic has
/// nothing left to ask for.
///
/// An operator expression directly under another operator expression
/// continues that one's chain, so the question is settled on the outermost
/// operator of the chain. A ternary root is never already named: its branches
/// are values, not the boolean parts the remediation asks to name.
fn chain_is_named(node_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    let mut root = node_id;
    loop {
        let parent = nodes.parent_id(root);
        match nodes.kind(parent) {
            AstKind::LogicalExpression(_) | AstKind::ConditionalExpression(_) => root = parent,
            _ => break,
        }
    }
    match nodes.kind(root) {
        AstKind::LogicalExpression(log) => {
            every_operand_is_named(&log.left) && every_operand_is_named(&log.right)
        }
        _ => false,
    }
}

/// What one source line accumulates: how many operators it carries, where the
/// leftmost one starts, and whether any of them still asks for a name.
struct LineOps {
    count: usize,
    anchor: Span,
    needs_names: bool,
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max_ops = ctx.config.threshold(super::META.id, "max_ops", ctx.lang);

        // Count logical (`&&`/`||`/`??`) and conditional (ternary `?`) operators
        // per source line from the AST. Each `LogicalExpression` node is one
        // operator — a chained `a && b && c` nests into two nodes, i.e. two
        // operators — and each `ConditionalExpression` node is one ternary.
        // Counting nodes rather than raw bytes means `?`/`&`/`|` characters
        // inside regex, string, and template literals never count: literal
        // content carries no operator nodes.
        //
        // Alongside the count, keep the span of the leftmost operator expression
        // on the line: the count decides whether to report, that span decides
        // where. Reporting the line at column 1 would point at the indentation.
        //
        // A line is left alone when every operator on it belongs to a chain
        // whose operands are all bare identifiers: the remediation asks for
        // names, and each operand already carries one.
        let mut ops_per_line: BTreeMap<usize, LineOps> = BTreeMap::new();
        for node in semantic.nodes().iter() {
            let span = match node.kind() {
                AstKind::LogicalExpression(expr) => expr.span,
                AstKind::ConditionalExpression(expr) => expr.span,
                _ => continue,
            };
            let (line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            let entry = ops_per_line.entry(line).or_insert(LineOps {
                count: 0,
                anchor: span,
                needs_names: false,
            });
            entry.count += 1;
            // Node iteration order is not specified, so keep the earliest
            // start seen rather than assuming the first one is it.
            if span.start < entry.anchor.start {
                entry.anchor = span;
            }
            // One operator that still asks for a name settles the line, so
            // stop walking chains once the line is known to report.
            if !entry.needs_names {
                entry.needs_names = !chain_is_named(node.id(), semantic.nodes());
            }
        }

        ops_per_line
            .into_values()
            .filter(|ops| ops.count >= max_ops && ops.needs_names)
            .map(|ops| {
                let span = ops.anchor;
                Diagnostic::at_offset(
                    Arc::clone(&ctx.path_arc),
                    ctx.source,
                    (span.start as usize, span.size() as usize),
                    super::META.id,
                    format!(
                        "Expression has {max_ops}+ logical/conditional operators — \
                         extract to named variables."
                    ),
                    Severity::Error,
                )
            })
            .collect()
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
    fn anchors_on_the_first_operator_of_the_line() {
        // Regression for rbaumier/comply#8386 — the rule groups by line by
        // design, but the anchor still belongs on the expression, not on the
        // indentation.
        let src = "  const x = a && b || c ?? d ? e : f;";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (1, 13));
        let (offset, len) = diags[0].span.expect("the anchor carries the expression's span");
        assert!(src[offset..offset + len].starts_with("a &&"));
    }

    #[test]
    fn flags_line_with_four_operators() {
        let src = "const x = a && b || c ?? d ? e : f;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_three_operators() {
        let src = "const x = a && b || c ? d : e;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_chaining() {
        let src = "const x = a?.b && c?.d || e;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_property_markers_in_type_literal() {
        // Phantom-key marker type — each `?: never` is the constraint, not a ternary.
        let src = "type ReservedFilterKeys = { page?: never; pageSize?: never; q?: never; sort?: never };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_function_parameter_markers() {
        // `?:` in a function signature marks optional params, not ternaries.
        let src = "function f(a?: T, b?: T, c?: T, d?: T): void {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_type_level_conditional_operators() {
        // A conditional *type* (`X extends string ? ... : ...`) and type-level
        // `&&`/`||`/`??` are not runtime `ConditionalExpression`/`LogicalExpression`
        // nodes, so they carry no operators to count. Counting via the AST (#6439)
        // — not raw bytes — correctly leaves this unflagged.
        let src = "type T<X> = X extends string ? A && B || C ?? D : E;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_tuple_element_markers() {
        // `'c'?` markers are optional tuple elements, not ternaries (issue #3318).
        let src = "expectType<readonly [undefined, 'c'?]>(getArrayTail(['a', undefined, 'c'] as readonly ['a', undefined, 'c'?]));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_optional_tuple_and_generic_type_markers() {
        // `(Set<string>)?`, `Set<string>?`, `number?`, `boolean?` are optional markers.
        let src = "expectType<[Set<string>, (Set<string>)?, Set<string>?]>({} as Schema<[string, number?, boolean?], Set<string>>);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_runtime_expression_with_four_real_operators() {
        // Genuine high-complexity runtime ternary/logical chain — must still fire.
        let src = "const x = a ? b : c || d && e ? f : g;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_real_operators_mixed_with_tuple_optional() {
        // One `T?` tuple marker is exempt, but the real operators alone still cross 4.
        let src = "const x = (y as [number?]) ? a && b || c ?? d : e;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_regex_quantifiers() {
        // Issue #6439: the `?` quantifiers in this regex (`-?`, `)?`, `[+-]?`) are
        // regex syntax, not ternaries — a regex literal carries no operator nodes.
        let src = r#"const JsonSigRx = /^\s*["[{]|^\s*-?\d{1,16}(\.\d{1,17})?([Ee][+-]?\d+)?\s*$/;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_string_literal_operators() {
        // `?`/`:`/`&&`/`||`/`??` inside a string literal are text, not operators.
        let src = r#"const s = "a ? b : c && d || e ?? f";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_template_literal_operators() {
        // Operator characters in a template literal's static text are not operators.
        let src = "const s = `a ? b : c && d || e ?? f`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_four_real_operators_with_valid_syntax() {
        // `a && b`, `c && d`, `||`, and the ternary `? :` — four operator nodes.
        let src = "const x = a && b || c && d ? e : f;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_three_real_operators_just_below_threshold() {
        // `a && b`, `(a && b) && c`, and the ternary `? :` — three operator nodes.
        let src = "const x = a && b && c ? d : e;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wrapped_multiline_operator_chain() {
        // A 4-operator chain split across lines is still one over-complex
        // expression: each `&&` is a `LogicalExpression` node attributed to the
        // expression's start line, so the line-wrapped form is flagged like the
        // single-line one.
        let src = "const ok =\n  a.p() &&\n  b.q() &&\n  c.r() &&\n  d.s() &&\n  e.t();";
        assert_eq!(run_on(src).len(), 1);
    }

    /// Regression for rbaumier/comply#8114 — every operand is already a named
    /// binding, so the remediation is applied and the diagnostic asks for
    /// nothing.
    #[test]
    fn allows_chain_whose_operands_are_all_named() {
        let src = "const ok = a && b && c && d && e;";
        assert!(run_on(src).is_empty());
    }

    /// `??` counts as a logical operator, so a chain of names reaches the
    /// threshold — and is left alone like the `&&` form.
    #[test]
    fn allows_nullish_chain_of_named_operands() {
        let src = "const ok = a ?? b ?? c ?? d ?? e;";
        assert!(run_on(src).is_empty());
    }

    /// A ternary's branches are values, not the boolean parts the remediation
    /// asks to name, so a ternary-rooted line keeps firing even when every
    /// identifier in it is bare.
    #[test]
    fn flags_ternary_rooted_line_of_named_operands() {
        let src = "const x = a && b && c && d ? e : f;";
        assert_eq!(run_on(src).len(), 1);
    }

    /// The guard covers the line, not one chain of it: a second chain with an
    /// inline operand keeps the line flagged.
    #[test]
    fn flags_line_whose_second_chain_has_an_inline_operand() {
        let src = "const a1 = p && q && r; const b1 = s && t && u.v();";
        assert_eq!(run_on(src).len(), 1);
    }
}
