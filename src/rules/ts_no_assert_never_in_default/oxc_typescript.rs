//! ts-no-assert-never-in-default OXC backend — flag `switch { default: throw ... }`
//! without an exhaustive `never` check.
//!
//! Suppressed when the switch discriminant cannot be narrowed to `never`: a
//! `for...of` / `for...in` loop element, or a binding annotated with a plain
//! primitive type (`string`, `number`, `boolean`, ...). On those, the `default:
//! throw` is runtime input validation, and `const _: never = x` would itself be
//! a TypeScript error — the exhaustiveness check is only meaningful for a union
//! or enum discriminant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType};
use std::sync::Arc;

pub struct Check;

const EXHAUSTIVE_MARKERS: &[&str] = &[
    "assertNever",
    "assertUnreachable",
    "exhaustiveCheck",
    "exhaustive(",
    ": never",
    "as never",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchStatement(switch) = node.kind() else { return };

        // A discriminant that can never be `never` (a `for...of`/`for...in`
        // element, or a primitive-annotated binding) makes the exhaustive check
        // a TypeScript error, not a fix — the `default: throw` is runtime input
        // validation. Only a union/enum discriminant goes stale on a new variant.
        if discriminant_cannot_be_never(&switch.discriminant, semantic) {
            return;
        }

        for case in &switch.cases {
            // default case has test == None
            if case.test.is_some() {
                continue;
            }
            let text = &ctx.source[case.span.start as usize..case.span.end as usize];
            if !text.contains("throw ") {
                continue;
            }
            if EXHAUSTIVE_MARKERS.iter().any(|m| text.contains(m)) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, case.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`switch` default throws without an exhaustive `never` check — adding a new \
                          union variant will pass the type-checker but hit this throw at runtime. \
                          Use `assertNever(x)` or `const _: never = x` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
            break;
        }
    }
}

/// True when `discriminant` resolves to a binding whose type can never narrow
/// to `never`, so an exhaustive `const _: never = x` check would not compile:
/// a `for...of`/`for...in` loop element, or a binding annotated with a plain
/// primitive keyword type.
fn discriminant_cannot_be_never<'a>(
    discriminant: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(ident) = discriminant else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);

    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        match kind {
            // Iterating a string/array/object yields elements/keys, never a
            // closed union. The declarator's parent is the for-of/for-in node,
            // so this is reached only after the no-annotation declarator below.
            AstKind::ForOfStatement(_) | AstKind::ForInStatement(_) => return true,
            AstKind::FormalParameter(param) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_primitive_type(&ann.type_annotation));
            }
            // A primitive-annotated binding settles it; otherwise keep walking
            // so an enclosing for-of/for-in is still detected.
            AstKind::VariableDeclarator(decl) => {
                if decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| is_primitive_type(&ann.type_annotation))
                {
                    return true;
                }
            }
            // Stop at the enclosing function/program — don't escape the binding.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

/// True for primitive keyword types that can hold infinitely many values and so
/// can never be exhausted to `never` (`string`, `number`, `boolean`, ...). A
/// union, enum, or named type reference is NOT primitive and keeps firing.
fn is_primitive_type(ty: &TSType) -> bool {
    matches!(
        ty,
        TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSBigIntKeyword(_)
            | TSType::TSSymbolKeyword(_)
            | TSType::TSObjectKeyword(_)
            | TSType::TSAnyKeyword(_)
            | TSType::TSUnknownKeyword(_)
    )
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_default_throw_over_union_param() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: throw new Error('unreachable'); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_default_with_assert_never() {
        let src = "function f(x: 'a' | 'b') { switch (x) { case 'a': return 1; case 'b': return 2; default: throw assertNever(x); } }";
        assert!(run(src).is_empty());
    }

    // Regression for #1092: switch over a `for...of` element (a plain string
    // character). `const _: never = c` would be a TypeScript error, so the
    // `default: throw` is runtime validation, not a stale exhaustiveness check.
    #[test]
    fn no_fp_switch_over_for_of_string_element_issue_1092() {
        let src = "export function fromString(resourceTypes: string) {\n\
                   for (const c of resourceTypes) {\n\
                   switch (c) {\n\
                   case \"s\": break;\n\
                   case \"c\": break;\n\
                   case \"o\": break;\n\
                   default: throw new RangeError(`Invalid resource type: ${c}`);\n\
                   }\n\
                   }\n\
                   }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn no_fp_switch_over_string_param() {
        let src = "function f(c: string) { switch (c) { case 'a': return 1; default: throw new Error(c); } }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn no_fp_switch_over_number_param() {
        let src = "function f(n: number) { switch (n) { case 1: return 1; default: throw new Error('bad'); } }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn no_fp_switch_over_for_in_key() {
        let src = "function f(obj: Record<string, number>) { for (const k in obj) { switch (k) { case 'a': break; default: throw new Error(k); } } }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_switch_over_union_local() {
        // A union-typed local binding stays flagged — it can go stale.
        let src = "function f() { const x: 'a' | 'b' = 'a' as 'a' | 'b'; switch (x) { case 'a': return 1; case 'b': return 2; default: throw new Error('x'); } }";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
