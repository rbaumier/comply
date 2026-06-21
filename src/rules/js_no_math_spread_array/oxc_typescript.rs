//! js-no-math-spread-array OXC backend — flag `Math.min(...arr)` / `Math.max(...arr)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_statically_bounded_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Math"])
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
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Math" {
            return;
        }
        let method = member.property.name.as_str();
        if method != "min" && method != "max" {
            return;
        }
        let spreads: Vec<&Expression> = call
            .arguments
            .iter()
            .filter_map(|a| match a {
                Argument::SpreadElement(s) => Some(&s.argument),
                _ => None,
            })
            .collect();
        if spreads.is_empty() {
            return;
        }
        // Spreading a statically-bounded array (literal, length-non-increasing
        // `.map`/`.filter`/`.slice` chain rooted at one, or a fixed-length tuple
        // binding) cannot exhaust the argument-count limit, so there is no
        // stack-overflow risk. Only flag when some spread operand is dynamic.
        if spreads
            .iter()
            .all(|operand| expression_is_statically_bounded_array(operand, semantic))
        {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`Math.{method}(...array)` overflows the stack on large arrays — \
                 use `reduce` or a for-loop instead."
            ),
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Dynamic / unbounded spreads — genuine stack-overflow risk, still flagged.
    #[test]
    fn flags_spread_of_dynamic_array_param() {
        assert_eq!(run_on("function f(nums: number[]) { return Math.max(...nums); }").len(), 1);
    }

    #[test]
    fn flags_spread_of_function_return() {
        assert_eq!(run_on("Math.max(...getList());").len(), 1);
    }

    #[test]
    fn flags_spread_of_unannotated_param() {
        assert_eq!(run_on("function f(xs) { return Math.min(...xs); }").len(), 1);
    }

    #[test]
    fn flags_map_rooted_at_dynamic_array() {
        assert_eq!(
            run_on("function f(nums: number[]) { return Math.max(...nums.map(x => x)); }").len(),
            1
        );
    }

    // Statically-bounded spreads — no stack risk, not flagged.
    #[test]
    fn allows_spread_of_array_literal() {
        assert!(run_on("Math.max(...[a, b, c]);").is_empty());
    }

    #[test]
    fn allows_spread_of_map_rooted_at_literal() {
        assert!(run_on("Math.min(...[a, b, c, d].map(c => c.x));").is_empty());
    }

    #[test]
    fn allows_spread_of_bounded_literal_binding() {
        let src = "const corners = [a, b, c, d]; Math.min(...corners.map(c => c.x));";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_spread_of_tuple_typed_binding() {
        let src = "const p: [number, number] = getP(); Math.max(...p);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Issue #5292: visgl/deck.gl — bbox corners spread to find an AABB.
    #[test]
    fn allows_deckgl_bbox_corners() {
        let src = r#"
            const transformedCoords = [
              modelMatrix.transformAsPoint([bbox[0], bbox[1]]),
              modelMatrix.transformAsPoint([bbox[2], bbox[1]]),
              modelMatrix.transformAsPoint([bbox[0], bbox[3]]),
              modelMatrix.transformAsPoint([bbox[2], bbox[3]]),
            ];
            const x = Math.min(...transformedCoords.map(i => i[0]));
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Issue #5292: visgl/deck.gl shadow.ts — frustum corners through chained maps.
    #[test]
    fn allows_deckgl_frustum_corners() {
        let src = r#"
            const corners = [[0,0,1],[1,0,1],[0,1,1],[1,1,1]].map(p => f(p));
            const positions = corners.map(c => g(c));
            const left = Math.min(...positions.map(p => p[0]));
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: `.flatMap` can grow the result beyond the bounded root, so
    // the spread is no longer provably bounded and must still flag.
    #[test]
    fn flags_flatmap_rooted_at_literal() {
        assert_eq!(run_on("Math.max(...[a, b].flatMap(x => x));").len(), 1);
    }

    // Negative space: a rest-element tuple `[number, ...number[]]` is unbounded,
    // so a binding typed as one must still flag.
    #[test]
    fn flags_rest_tuple_typed_binding() {
        let src = "const xs: [number, ...number[]] = getXs(); Math.max(...xs);";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: a `let` binding can be reassigned to a dynamic array after
    // its bounded literal initializer, so the literal arity is not load-bearing.
    #[test]
    fn flags_reassigned_let_binding() {
        let src = "let arr = [a, b]; arr = getHuge(); Math.max(...arr);";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: a `const` array literal grown in place via `.push` is an
    // accumulator with unknown final size, so it must still flag.
    #[test]
    fn flags_push_accumulator() {
        let src = "const arr = []; for (const x of xs) arr.push(x); Math.max(...arr);";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
