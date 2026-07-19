//! prefer-set-size OXC backend — flag `[...set].length` and `Array.from(set).length`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_map, expression_is_set};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
use std::sync::Arc;

pub struct Check;

/// The operand of a single-spread array literal (`[...X]`) or of `Array.from(X)`
/// — the value whose length is being read. `None` for any other object
/// expression, an array literal that is not exactly one spread element, or an
/// `Array.from` call with no first argument.
fn spread_or_array_from_operand<'a>(obj: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    match obj {
        Expression::ArrayExpression(arr) => {
            let non_elision = arr.elements.iter().filter(|e| !e.is_elision()).count();
            if non_elision != 1 {
                return None;
            }
            arr.elements.iter().find_map(|el| match el {
                ArrayExpressionElement::SpreadElement(s) => Some(&s.argument),
                _ => None,
            })
        }
        Expression::CallExpression(call) => {
            let is_array_from = matches!(&call.callee, Expression::StaticMemberExpression(m)
                if m.property.name == "from"
                && matches!(&m.object, Expression::Identifier(id) if id.name == "Array"));
            if !is_array_from {
                return None;
            }
            call.arguments.first().and_then(|arg| arg.as_expression())
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else { return };

        if member.property.name != "length" {
            return;
        }

        let Some(operand) = spread_or_array_from_operand(&member.object) else {
            return;
        };

        // `.length → .size` is a valid rewrite only when the spread/`Array.from`
        // operand is provably a `Set`/`Map`. Spreading a string (`[...String(v)]`,
        // `[...'abc']`), an array, or an unresolved binding produces a value with
        // no `.size`, so those are left alone.
        if !expression_is_set(operand, semantic) && !expression_is_map(operand, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Set#size` instead of `[...set].length` or `Array.from(set).length`.".into(),
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
    fn flags_spread_new_set() {
        let src = "[...new Set([1, 2, 2])].length;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_spread_new_set_binding() {
        // `s` resolves to a `new Set()` initializer, so `.length → .size` holds.
        let src = "const s = new Set<number>();\n[...s].length;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_spread_set_typed_binding() {
        // `s` carries a `Set<number>` annotation even though its initializer is
        // an opaque call.
        let src = "const s: Set<number> = getSet();\n[...s].length;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_array_from_new_map() {
        let src = "Array.from(new Map()).length;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_array_from_new_set() {
        let src = "Array.from(new Set([1])).length;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_typed_parameter() {
        // The `FormalParameter` resolution path: a parameter annotated `Set<T>`.
        let src = "function f(s: Set<number>) { return [...s].length; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_multi_element_spread_with_set() {
        // Two non-elision elements is not the `[...set]` shape, so the operand is
        // never inspected — no rewrite to `.size` is possible.
        let src = "[x, ...new Set()].length;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_array_from_no_args() {
        let src = "Array.from().length;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_spread_string_call() {
        // `[...String(value)]` spreads a string into an array to count Unicode
        // code points — a string has no `.size`, so the rewrite would not
        // compile (#7245).
        let src = "[...String(value)].length;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_spread_string_literal() {
        let src = "[...'abc'].length;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_spread_array_identifier() {
        // A binding initialised from an array literal is not a Set/Map.
        let src = "const someArray = [1, 2];\n[...someArray].length;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_spread_unresolved_identifier() {
        let src = "[...unresolved].length;";
        assert!(run_on(src).is_empty());
    }
}
