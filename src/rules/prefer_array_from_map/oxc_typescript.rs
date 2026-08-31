//! prefer-array-from-map oxc backend — flag `[...x].map(fn)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, expression_is_array, expression_is_map, expression_is_set,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["map"])
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
        if member.property.name.as_str() != "map" {
            return;
        }

        // Receiver must be `[...operand]` — a single spread, nothing else.
        let Expression::ArrayExpression(arr) = &member.object else {
            return;
        };
        let [ArrayExpressionElement::SpreadElement(spread)] = arr.elements.as_slice() else {
            return;
        };
        let operand = &spread.argument;

        // The two ends of the spread carry different fixes, and an operand whose
        // type neither branch can prove carries neither: an unknown iterable may
        // have no `.map` of its own, and an unknown `.map` holder may not be
        // iterable, so both rewrites would be guesses.
        let message = if expression_is_array(operand, semantic) {
            "`.map()` already returns a new array — drop the `[...]` copy."
        } else if expression_is_map(operand, semantic) || expression_is_set(operand, semantic) {
            "Use `Array.from(iter, mapFn)` instead of `[...iter].map(mapFn)`."
        } else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
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
    fn flags_spread_of_set_with_array_from() {
        let d = run_on("const ids = new Set<string>();\nconst out = [...ids].map((x) => x.length);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn flags_spread_of_map_with_array_from() {
        let d = run_on(
            "const byId = new Map<string, number>();\nconst out = [...byId].map(([k]) => k);",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn flags_spread_of_array_by_dropping_the_copy() {
        let d = run_on("const arr: number[] = [1, 2];\nconst out = [...arr].map((x) => x + 1);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("drop the"));
    }

    #[test]
    fn does_not_flag_unproven_operand() {
        let d = run_on("export function f(input) {\n  return [...input].map((x) => x);\n}");
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn does_not_flag_map_without_spread() {
        let d = run_on("const arr: number[] = [1, 2];\nconst out = arr.map((x) => x + 1);");
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn does_not_flag_spread_mixed_with_other_elements() {
        let d = run_on("const arr: number[] = [1];\nconst out = [...arr, 3].map((x) => x + 1);");
        assert_eq!(d.len(), 0);
    }
}
