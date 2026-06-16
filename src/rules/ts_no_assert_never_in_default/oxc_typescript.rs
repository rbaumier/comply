//! ts-no-assert-never-in-default OXC backend — flag `switch { default: throw ... }`
//! without an exhaustive `never` check.
//!
//! Suppressed when the switch discriminant cannot be narrowed to `never`: a
//! `for...of` / `for...in` loop element, a binding annotated with a plain
//! primitive type (`string`, `number`, `boolean`, ...), or a binding whose
//! initializer calls a same-file function declared to return such a primitive.
//! On those, the `default: throw` is runtime input validation, and
//! `const _: never = x` would itself be a TypeScript error — the exhaustiveness
//! check is only meaningful for a union or enum discriminant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference, TSType};
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
/// a `for...of`/`for...in` loop element, a binding annotated with a plain
/// primitive keyword type, or a binding initialized by a same-file function
/// call whose declared return type is such a primitive (inferred-primitive
/// binding).
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
            // A primitive-annotated binding settles it; otherwise an
            // initializer calling a same-file function declared to return a
            // primitive gives the binding an inferred primitive type, which is
            // equally un-narrowable to `never`. Either way, keep walking so an
            // enclosing for-of/for-in is still detected.
            AstKind::VariableDeclarator(decl) => {
                if declarator_is_primitive(decl, semantic) {
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

/// True when the `let`/`const` binding has a primitive type: either an explicit
/// primitive type annotation, or no annotation but an initializer calling a
/// same-file function declared to return a primitive (inferred primitive).
fn declarator_is_primitive<'a>(
    decl: &oxc_ast::ast::VariableDeclarator<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if let Some(ann) = decl.type_annotation.as_ref() {
        return is_primitive_type(&ann.type_annotation);
    }
    if let Some(Expression::CallExpression(call)) = decl.init.as_ref()
        && let Expression::Identifier(callee) = &call.callee
    {
        return callee_returns_primitive(callee, semantic);
    }
    false
}

/// True when `callee` resolves to a same-file function (declaration, arrow, or
/// function expression) whose explicit return-type annotation is a plain
/// primitive keyword. The symbol table picks the binding in scope at the call
/// site, so shadowing is respected. Returns false when the callee resolves to
/// no in-file binding, or the return type is absent or a union/literal/named
/// type — those still narrow to `never` and keep firing.
fn callee_returns_primitive<'a>(
    callee: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let scoping = semantic.scoping();
    let Some(ref_id) = callee.reference_id.get() else {
        return false;
    };
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(sym_id);
    let decl_kind = std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find(|kind| matches!(kind, AstKind::Function(_) | AstKind::VariableDeclarator(_)));

    let return_type = match decl_kind {
        // `function f(): string { ... }`
        Some(AstKind::Function(func)) => func.return_type.as_ref(),
        // `const f = (): string => ...` / `const f = function (): string {}`
        Some(AstKind::VariableDeclarator(decl)) => match decl.init.as_ref() {
            Some(Expression::ArrowFunctionExpression(arrow)) => arrow.return_type.as_ref(),
            Some(Expression::FunctionExpression(func)) => func.return_type.as_ref(),
            _ => return false,
        },
        _ => return false,
    };
    return_type.is_some_and(|ann| is_primitive_type(&ann.type_annotation))
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

    // Regression for #3291: discriminant is a `const` initialized by a same-file
    // function declared to return `string` (inferred-primitive). `const _: never
    // = version` would be a TypeScript error, so the `default: throw` is runtime
    // validation, not a stale exhaustiveness check.
    #[test]
    fn no_fp_inferred_primitive_from_function_return_issue_3291() {
        let src = "function determinePayloadFormat(event: object): string { return '1.0'; }\n\
                   const version = determinePayloadFormat({});\n\
                   switch (version) {\n\
                   case '1.0': break;\n\
                   case '2.0': break;\n\
                   default: throw new Error(`Unsupported version: ${version}`);\n\
                   }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn no_fp_inferred_primitive_from_arrow_return() {
        let src = "const detect = (): string => '1.0';\n\
                   const version = detect();\n\
                   switch (version) {\n\
                   case '1.0': break;\n\
                   default: throw new Error(version);\n\
                   }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A plain `.js` file has no TypeScript types, so the rule is not registered
    // for JavaScript and must produce no diagnostics.
    #[test]
    fn no_fp_in_js_file_issue_3291() {
        let src = "function determinePayloadFormat(event) { return '1.0'; }\n\
                   const version = determinePayloadFormat({});\n\
                   switch (version) {\n\
                   case '1.0': break;\n\
                   default: throw new Error('Unsupported version: ' + version);\n\
                   }";
        let diags =
            crate::rules::test_helpers::run_rule_by_id(crate::rules::ts_no_assert_never_in_default::META.id, src, "env.js");
        assert!(diags.is_empty(), "{diags:?}");
    }

    // A same-file function returning a string-literal union IS narrowable to
    // `never` after the cases are handled, so the suppression must NOT apply —
    // the rule still fires (the genuine stale-exhaustiveness case).
    #[test]
    fn still_flags_inferred_literal_union_from_function_return() {
        let src = "function detect(): '1.0' | '2.0' { return '1.0'; }\n\
                   const version = detect();\n\
                   switch (version) {\n\
                   case '1.0': break;\n\
                   case '2.0': break;\n\
                   default: throw new Error(version);\n\
                   }";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
