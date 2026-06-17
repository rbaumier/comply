//! no-array-callback-reference OXC backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.
//!
//! Only single-argument iterator calls are flagged; multi-argument calls
//! (data-first functional APIs like fp-ts `Module.map(value, fn)`, or an
//! explicit `thisArg`) are exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, FormalParameters};
use std::sync::Arc;

/// Returns `true` when a callee's formal parameter list cannot bind the extra
/// `(index, array)` arguments an iterator method injects after `element` to a
/// *positional* parameter — so passing it bare is identical to wrapping it:
///   - zero positional params with a rest (`(...rest) => …`) is a sink that
///     ignores everything (#825);
///   - zero positional params (`() => x`) ignore every argument;
///   - one positional param and no rest (`(str) => …`) binds only `element`
///     and silently drops `index`/`array`, so `arr.map(f)` is identical to
///     `arr.map(e => f(e))` (#3901).
/// A positional parameter *followed* by a rest (`(x, ...rest)`) captures
/// `index` in `rest`, and two or more positional params expose the genuine
/// `parseInt(string, radix)` footgun where `index` becomes the second
/// argument — neither is exempt.
fn callee_ignores_extra_args(params: &FormalParameters) -> bool {
    match params.items.len() {
        0 => true,
        1 => params.rest.is_none(),
        _ => false,
    }
}

/// Returns `true` when `ident` resolves to a locally-declared function whose
/// formal parameter list ignores the extra iterator arguments
/// (see [`callee_ignores_extra_args`]). Cross-file imports do not resolve here
/// (`symbol_id() == None`) and stay flagged, matching the rule's conservative
/// default.
fn is_low_arity_local<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    match nodes.kind(decl_node_id) {
        AstKind::VariableDeclarator(decl) => match decl.init.as_ref() {
            Some(Expression::ArrowFunctionExpression(f)) => {
                callee_ignores_extra_args(&f.params)
            }
            Some(Expression::FunctionExpression(f)) => callee_ignores_extra_args(&f.params),
            _ => false,
        },
        AstKind::Function(f) => callee_ignores_extra_args(&f.params),
        _ => false,
    }
}

/// Returns `true` when `name` follows the PascalCase convention reserved for
/// types, classes and constructors (leading uppercase, contains a lowercase
/// letter). A PascalCase reference passed as the sole argument to a
/// `find`/`map`/`flatMap` call is a node-type/constructor — e.g. jscodeshift
/// `Collection.find(NodeType)` — not a per-element `(value, index, array)`
/// transform, so wrapping it in an arrow function would be wrong. Screaming
/// SNAKE_CASE constants (no lowercase) are excluded so they stay flagged.
fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    first.is_ascii_uppercase() && name.chars().any(|c| c.is_ascii_lowercase())
}

pub struct Check;

const ITERATOR_METHODS: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "reduce",
    "reduceRight",
    "some",
];

const IGNORED_IDENTIFIERS: &[&str] = &["Boolean", "String", "Number", "BigInt", "Symbol"];

impl OxcCheck for Check {
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

        // Must be a member expression call: `something.method(callback)`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !ITERATOR_METHODS.contains(&method_name) {
            return;
        }

        // The accidental-callback-reference footgun (`arr.map(parseInt)`) is always a
        // single-argument call. A second argument means a data-first functional API
        // (fp-ts `Module.map(value, fn)`, Ramda, …) where arg0 is the value, or an
        // explicit `thisArg` the author deliberately bound — neither is the footgun.
        if call.arguments.len() != 1 {
            return;
        }

        // Get the first argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        match expr {
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if IGNORED_IDENTIFIERS.contains(&name) {
                    return;
                }
                // A PascalCase reference is a type/class/constructor, not a
                // per-element transform — e.g. jscodeshift `root.find(NodeType)`.
                if is_pascal_case(name) {
                    return;
                }
                if is_low_arity_local(ident, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass function `{name}` directly to `.{method_name}(…)` — use `(…) => {name}(…)` instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            Expression::StaticMemberExpression(inner_member) => {
                // A PascalCase property is a node-type/constructor reference
                // (jscodeshift `root.find(j.ExportNamedDeclaration)`), not a
                // per-element transform callback.
                if is_pascal_case(inner_member.property.name.as_str()) {
                    return;
                }
                let text = &ctx.source
                    [inner_member.span.start as usize..inner_member.span.end as usize];
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, inner_member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass `{text}` directly to `.{method_name}(…)` — wrap it in an arrow function."
                    ),
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
    use super::Check;

    fn run_on(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression #1032: fp-ts data-first call — arg0 is the monadic value, not a callback.
    #[test]
    fn no_fp_data_first_two_arg_call() {
        assert!(run_on("const a = MT.map(greetingT, (s: string) => s + '!');").is_empty());
    }

    #[test]
    fn no_fp_function_reference_with_this_arg() {
        assert!(run_on("const g = arr.map(this.handler, this);").is_empty());
    }

    #[test]
    fn flags_single_arg_identifier_reference() {
        assert_eq!(run_on("const x = arr.map(parseInt);").len(), 1);
    }

    #[test]
    fn flags_single_arg_local_function_reference() {
        assert_eq!(run_on("const x = arr.filter(myFunc);").len(), 1);
    }

    #[test]
    fn flags_single_arg_member_reference() {
        assert_eq!(run_on("const x = arr.map(utils.transform);").len(), 1);
    }

    #[test]
    fn no_fp_arrow_callback() {
        assert!(run_on("const x = arr.map(x => parseInt(x));").is_empty());
    }

    #[test]
    fn no_fp_boolean_constructor() {
        assert!(run_on("const x = arr.filter(Boolean);").is_empty());
    }

    // Regression #825 — zero-param and rest-only local callbacks safely ignore extra args.
    #[test]
    fn allows_zero_arity_arrow_function() {
        assert!(run_on("const c = () => 'x'; const arr: string[] = []; arr.map(c);").is_empty());
    }

    #[test]
    fn allows_zero_arity_function_expression() {
        assert!(run_on(
            "const c = function() { return 'x'; }; const arr: string[] = []; arr.map(c);"
        )
        .is_empty());
    }

    #[test]
    fn allows_zero_arity_function_declaration() {
        assert!(
            run_on("function c() { return 'x'; } const arr: string[] = []; arr.map(c);")
                .is_empty()
        );
    }

    #[test]
    fn allows_rest_only_function() {
        assert!(run_on(
            "const c = (..._a: any[]) => undefined; const arr: string[] = []; arr.map(c);"
        )
        .is_empty());
    }

    // #3901: a single-parameter callee binds only `element` and silently drops
    // the injected `index`/`array` args, so `arr.map(c)` is identical to
    // `arr.map(e => c(e))` — exempt, exactly like the zero-arity case.
    #[test]
    fn allows_single_param_callback() {
        assert!(
            run_on("const c = (x: number) => x * 2; const arr: number[] = []; arr.map(c);")
                .is_empty()
        );
    }

    // #3901 repro: a single-parameter function declaration passed to `.map`.
    #[test]
    fn allows_single_param_function_declaration() {
        assert!(run_on(
            "function trim(s: string): string { return s.trim(); } const from: string = ''; from.split('.').map(trim);"
        )
        .is_empty());
    }

    // #3901 repro: kysely `.some(isExpressionOrFactory)` single-param type-guard.
    #[test]
    fn allows_single_param_type_guard() {
        assert!(run_on(
            "function isExpressionOrFactory(o: unknown): o is string { return typeof o === 'string'; } const arg: unknown[] = []; arg.some(isExpressionOrFactory);"
        )
        .is_empty());
    }

    // #3901 negative space: a two-parameter callee CAN receive `index` as a
    // second positional argument (the `parseInt(string, radix)` footgun) —
    // it must stay flagged.
    #[test]
    fn flags_two_param_function_declaration() {
        assert_eq!(
            run_on("function f(a: number, b: number) { return a + b; } const arr: number[] = []; arr.map(f);").len(),
            1
        );
    }

    // #3901 negative space: a rest parameter following a positional one
    // (`(x, ...rest)`) captures the injected `index`/`array` in `rest`, so the
    // footgun applies — keep it flagged (`rest.is_some()` → not exempt).
    #[test]
    fn flags_param_plus_rest_function() {
        assert_eq!(
            run_on(
                "const c = (_x: number, ..._r: number[]) => 0; const arr: number[] = []; arr.map(c);"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_imported_function_conservatively() {
        // Cross-file import: symbol_id() is None → conservative, must flag.
        assert_eq!(
            run_on("import { importedFn } from './other'; const arr: string[] = []; arr.map(importedFn);").len(),
            1
        );
    }

    // Regression #1194: jscodeshift `Collection.find(NodeType, filter)` — a
    // node-type constructor first argument, often with a filter as the second.
    #[test]
    fn no_jscodeshift_find_two_arg_node_type() {
        assert!(run_on(
            "root.find(j.ExportNamedDeclaration, { declaration: { type: 'VariableDeclaration' } });"
        )
        .is_empty());
    }

    // Regression #1194: jscodeshift single-arg node-type via member expression.
    #[test]
    fn no_jscodeshift_find_member_node_type() {
        assert!(run_on("root.find(j.ExportNamedDeclaration);").is_empty());
    }

    // Regression #1194: bare PascalCase node-type / constructor reference.
    #[test]
    fn no_pascal_case_identifier_reference() {
        assert!(run_on("root.find(ExportNamedDeclaration);").is_empty());
    }

    // Negative-space guard #1194: a lower-camelCase function reference is still
    // the array-callback footgun and must stay flagged.
    #[test]
    fn flags_camel_case_function_reference() {
        assert_eq!(run_on("const x = items.map(transform);").len(), 1);
        assert_eq!(run_on("const x = users.find(isActive);").len(), 1);
    }
}
