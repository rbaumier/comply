//! no-array-callback-reference OXC backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.
//!
//! Only single-argument iterator calls are flagged; multi-argument calls
//! (data-first functional APIs like fp-ts `Module.map(value, fn)`, or an
//! explicit `thisArg`) are exempt. Calls whose receiver is a namespace-import
//! binding (`import * as O from 'fp-ts/Option'; O.some(n)`) are also exempt:
//! those are data-library combinators, never `Array.prototype.<method>`.
//! Bare references to a local callee — or a parameter/variable typed as a
//! single-arity function (`scale: (x: number) => number`), including one
//! destructured from a typed params object (`{ scale }: Params`) — that binds
//! only the `element` argument are exempt: passing them directly is identical to
//! wrapping them in an arrow. A `this.method` reference is likewise exempt when
//! `method` is an arrow-function class property (auto-bound to `this`) declaring
//! at most one parameter. An argument that resolves to a `for...in` loop
//! variable is exempt too: such a key is always a `string`, never a function.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ClassElement, Expression, ForStatementLeft, FormalParameters, StaticMemberExpression,
    TSSignature, TSType, TSTypeAnnotation, TSTypeName,
};
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

/// Returns `true` when a type annotation is a function type that ignores the
/// extra iterator arguments — i.e. its declared signature binds only `element`
/// (see [`callee_ignores_extra_args`]). A parameter or variable typed
/// `(value: number) => string` is statically known to receive at most one
/// argument, so passing it bare to `.map`/`.filter` is type-safe. Parenthesized
/// types (`((x: T) => R)`) are unwrapped. Opaque type references
/// (`Scale<number, number>`) carry no visible arity and are not exempt here.
fn is_low_arity_function_type(ty: &TSType) -> bool {
    match ty {
        TSType::TSFunctionType(f) => callee_ignores_extra_args(&f.params),
        TSType::TSParenthesizedType(p) => is_low_arity_function_type(&p.type_annotation),
        _ => false,
    }
}

/// Returns `true` when an object-type member list declares `binding_name` as a
/// single-arity function — covering both `{ f: (x) => y }`
/// (`TSPropertySignature`) and the method-shorthand `{ f(x): y }`
/// (`TSMethodSignature`).
fn members_declare_low_arity(members: &[TSSignature], binding_name: &str) -> bool {
    members.iter().any(|member| match member {
        TSSignature::TSPropertySignature(prop) => {
            prop.key.static_name().as_deref() == Some(binding_name)
                && prop
                    .type_annotation
                    .as_ref()
                    .is_some_and(|a| is_low_arity_function_type(&a.type_annotation))
        }
        TSSignature::TSMethodSignature(method) => {
            method.key.static_name().as_deref() == Some(binding_name)
                && callee_ignores_extra_args(&method.params)
        }
        _ => false,
    })
}

/// Returns `true` when a named type reference (`Params` in `{ scale }: Params`)
/// resolves to a `type`/`interface` declaration whose `binding_name` member is a
/// single-arity function. The declaration is matched by name across the module —
/// the established resolution shape in this codebase (see
/// `ts_no_enum_object_literal_pattern`). Generic type references carrying their
/// own arguments are skipped: the member type may depend on a type parameter
/// whose arity is not statically visible here.
fn named_type_member_is_low_arity<'a>(
    type_ref: &oxc_ast::ast::TSTypeReference<'a>,
    binding_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if type_ref.type_arguments.is_some() {
        return false;
    }
    let TSTypeName::IdentifierReference(id) = &type_ref.type_name else { return false };
    let type_name = id.name.as_str();
    semantic.nodes().iter().any(|node| match node.kind() {
        AstKind::TSTypeAliasDeclaration(alias) if alias.id.name.as_str() == type_name => {
            matches!(&alias.type_annotation, TSType::TSTypeLiteral(lit)
                if members_declare_low_arity(&lit.members, binding_name))
        }
        AstKind::TSInterfaceDeclaration(iface) if iface.id.name.as_str() == type_name => {
            members_declare_low_arity(&iface.body.body, binding_name)
        }
        _ => false,
    })
}

/// Returns `true` when a parameter's type annotation declares `binding_name` as
/// a single-arity function. Covers a direct annotation (`scale: (x) => y`), the
/// destructured inline-object case (`{ scale }: { scale: (x) => y }`), and the
/// destructured named-type case (`{ scale }: Params`) — the common D3/charting
/// shape where scales and formatters are destructured from a typed params
/// object.
fn param_binding_is_low_arity<'a>(
    ann: &TSTypeAnnotation<'a>,
    binding_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match &ann.type_annotation {
        TSType::TSTypeLiteral(lit) => members_declare_low_arity(&lit.members, binding_name),
        TSType::TSTypeReference(type_ref) => {
            named_type_member_is_low_arity(type_ref, binding_name, semantic)
        }
        ty => is_low_arity_function_type(ty),
    }
}

/// Returns `true` when `ident` resolves to a locally-declared function whose
/// formal parameter list ignores the extra iterator arguments
/// (see [`callee_ignores_extra_args`]), or to a parameter/variable whose type
/// annotation is a single-arity function type (see
/// [`is_low_arity_function_type`]). Cross-file imports do not resolve here
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
        AstKind::VariableDeclarator(decl) => {
            // A `for...in` loop variable (`for (const key in obj) arr.map(key)`)
            // is, per the ECMAScript spec, always a `string` — never a function
            // reference — so passing it bare cannot be the `arr.map(parseInt)`
            // footgun. Its binding is a `VariableDeclarator` whose
            // `VariableDeclaration` is the `left` head of a `ForInStatement` —
            // matched by span so a `var` in an unbraced loop body does not
            // qualify. A `for...of` element binding is excluded: it can
            // legitimately hold a function reference.
            let var_decl_node = nodes.parent_node(decl_node_id);
            if let AstKind::VariableDeclaration(var_decl) = var_decl_node.kind()
                && let AstKind::ForInStatement(for_in) =
                    nodes.parent_node(var_decl_node.id()).kind()
                && let ForStatementLeft::VariableDeclaration(head) = &for_in.left
                && head.span == var_decl.span
            {
                return true;
            }
            if let Some(ann) = decl.type_annotation.as_ref()
                && is_low_arity_function_type(&ann.type_annotation)
            {
                return true;
            }
            match decl.init.as_ref() {
                Some(Expression::ArrowFunctionExpression(f)) => {
                    callee_ignores_extra_args(&f.params)
                }
                Some(Expression::FunctionExpression(f)) => callee_ignores_extra_args(&f.params),
                _ => false,
            }
        }
        AstKind::Function(f) => callee_ignores_extra_args(&f.params),
        // A function parameter resolves to its `BindingIdentifier` (or, for a
        // bare param, the `FormalParameter` itself); the enclosing
        // `FormalParameter` carries the type annotation. A parameter typed as a
        // single-arity function (`scale: (x: number) => number`), directly or as
        // a destructured property of a typed params object, is safe to pass bare.
        _ => {
            let binding_name = scoping.symbol_name(sym_id);
            std::iter::once(nodes.kind(decl_node_id))
                .chain(nodes.ancestor_kinds(decl_node_id))
                .any(|kind| match kind {
                    AstKind::FormalParameter(param) => {
                        param.type_annotation.as_ref().is_some_and(|ann| {
                            param_binding_is_low_arity(ann, binding_name, semantic)
                        })
                    }
                    _ => false,
                })
        }
    }
}

/// Returns `true` when `ident` resolves to a namespace-import binding
/// (`import * as X from '…'`). A `X.method(...)` call on such a binding is a
/// data-library combinator (fp-ts `O.some(v)` / `O.map(v)`), never
/// `Array.prototype.<method>`, so its argument is a value to wrap, not a
/// per-element callback. Resolution mirrors [`is_low_arity_local`]:
/// `reference_id` → symbol → declaration node, which for a namespace import is
/// the `ImportNamespaceSpecifier` itself.
fn is_namespace_import_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl = scoping.symbol_declaration(sym_id);
    matches!(semantic.nodes().kind(decl), AstKind::ImportNamespaceSpecifier(_))
}

/// Returns `true` when `member` is `this.<method>` and `<method>` resolves, in
/// the nearest enclosing class body, to an arrow-function class property whose
/// formal parameter list ignores the extra iterator arguments
/// (see [`callee_ignores_extra_args`]). An arrow class property
/// (`private m = (x) => …`) is auto-bound to `this` at construction, so passing
/// `this.m` bare keeps `this`; a single declared parameter then drops the
/// injected `index`/`array`, making `arr.map(this.m)` identical to
/// `arr.map(e => this.m(e))`. A normal (non-arrow) method loses `this` when
/// passed bare, and a multi-arity arrow exposes the extra-args footgun — neither
/// is exempt. This mirrors [`is_low_arity_local`] for the `this.method` form.
fn is_low_arity_bound_class_property<'a>(
    member: &StaticMemberExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if !matches!(member.object, Expression::ThisExpression(_)) {
        return false;
    }
    let prop_name = member.property.name.as_str();
    let nodes = semantic.nodes();
    for kind in nodes.ancestor_kinds(node.id()) {
        if let AstKind::Class(class) = kind {
            return class.body.body.iter().any(|element| match element {
                ClassElement::PropertyDefinition(prop) => {
                    prop.key.static_name().as_deref() == Some(prop_name)
                        && matches!(
                            prop.value.as_ref(),
                            Some(Expression::ArrowFunctionExpression(f))
                                if callee_ignores_extra_args(&f.params)
                        )
                }
                _ => false,
            });
        }
    }
    false
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

        // `import * as O from 'fp-ts/Option'; O.some(n)` is `Option.some` — a
        // constructor wrapping `n`, not `Array.prototype.some`. A receiver that
        // resolves to a namespace import is a data-library combinator, so its
        // argument is a value, never a callback.
        if let Expression::Identifier(obj) = &member.object
            && is_namespace_import_binding(obj, semantic)
        {
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
                // `this.method` where `method` is an auto-bound arrow class
                // property declaring at most one parameter keeps `this` and
                // drops the injected `index`/`array`, so passing it bare is safe.
                if is_low_arity_bound_class_property(inner_member, node, semantic) {
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

    // #4764: a function parameter typed as a single-arity callback
    // (`formatValue: (value: number) => string`) binds only `element`, so
    // passing it bare to `.map` is type-safe — exempt.
    #[test]
    fn allows_single_arity_typed_parameter() {
        assert!(run_on(
            "const f = ({ formatValue }: { formatValue: (value: number) => string }) => { const values: number[] = []; return values.map(formatValue); };"
        )
        .is_empty());
    }

    // #4764 repro (plouc/nivo): destructuring a single-arity callback from a
    // named params type (`{ formatValue }: Params`) must not flag `.map`.
    #[test]
    fn allows_single_arity_named_type_alias() {
        assert!(run_on(
            "type Params = { formatValue: (value: number) => string }; const f = ({ formatValue }: Params) => { const values: number[] = []; return values.map(formatValue); };"
        )
        .is_empty());
    }

    // #4764: a named interface resolves the same way as a type alias.
    #[test]
    fn allows_single_arity_named_interface() {
        assert!(run_on(
            "interface Params { formatValue: (value: number) => string } const f = ({ formatValue }: Params) => { const values: number[] = []; return values.map(formatValue); };"
        )
        .is_empty());
    }

    // #4764: method-shorthand signature (`{ f(x): y }`) in a named type is also
    // a single-arity callback.
    #[test]
    fn allows_single_arity_method_shorthand() {
        assert!(run_on(
            "type Params = { formatValue(value: number): string }; const f = ({ formatValue }: Params) => { const values: number[] = []; return values.map(formatValue); };"
        )
        .is_empty());
    }

    // #4764 negative space: a multi-arity member in a named type stays flagged.
    #[test]
    fn flags_two_arity_named_type_member() {
        assert_eq!(
            run_on(
                "type Params = { cb: (a: number, b: number) => number }; const f = ({ cb }: Params) => { const values: number[] = []; return values.map(cb); };"
            )
            .len(),
            1
        );
    }

    // #4764: a parenthesized single-arity function type annotation is unwrapped.
    #[test]
    fn allows_single_arity_typed_parameter_parenthesized() {
        assert!(run_on(
            "const f = (formatValue: ((value: number) => string)) => { const values: number[] = []; return values.map(formatValue); };"
        )
        .is_empty());
    }

    // #4764: a single-arity function type on a variable annotation is exempt too.
    #[test]
    fn allows_single_arity_typed_variable() {
        assert!(run_on(
            "const scale: (x: number) => number = getScale(); const values: number[] = []; values.map(scale);"
        )
        .is_empty());
    }

    // #4764 negative space: a two-parameter typed callback exposes the
    // `(element, index)` footgun and must stay flagged.
    #[test]
    fn flags_two_arity_typed_parameter() {
        assert_eq!(
            run_on(
                "const f = (cb: (a: number, b: number) => number) => { const values: number[] = []; return values.map(cb); };"
            )
            .len(),
            1
        );
    }

    // #4764 negative space: an opaque type reference (`Scale<number, number>`)
    // carries no visible arity, so a bare reference stays flagged.
    #[test]
    fn flags_opaque_typed_parameter() {
        assert_eq!(
            run_on(
                "const f = (scale: Scale<number, number>) => { const values: number[] = []; return values.map(scale); };"
            )
            .len(),
            1
        );
    }

    // #4764 negative space: an untyped parameter has no annotation to inspect —
    // the conservative default still flags it (`arr.map(parseInt)`-class bug).
    #[test]
    fn flags_untyped_parameter() {
        assert_eq!(
            run_on(
                "const f = (cb) => { const values: number[] = []; return values.map(cb); };"
            )
            .len(),
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

    // Regression #4469: fp-ts `O.some(n)` is `Option.some`, wrapping `n` in a
    // `Some` container — not `Array.prototype.some`. The receiver resolves to a
    // namespace import, so the argument is a value, not a callback.
    #[test]
    fn no_fp_ts_namespace_some_constructor() {
        assert!(run_on(
            "import * as O from 'fp-ts/Option'; const f = (n: number) => O.some(n);"
        )
        .is_empty());
    }

    // #4469: `O.map(double)` on a namespace import is a combinator, not array map.
    #[test]
    fn no_fp_ts_namespace_map_combinator() {
        assert!(
            run_on("import * as O from 'fp-ts/Option'; const g = O.map(double);").is_empty()
        );
    }

    // #4469: `A.filter(pred)` on a namespace import is a combinator, not array filter.
    #[test]
    fn no_fp_ts_namespace_filter_combinator() {
        assert!(run_on("import * as A from 'fp-ts/Array'; A.filter(pred);").is_empty());
    }

    // #4469 negative space: a local array receiver is NOT a namespace import, so
    // the real `arr.map(parseInt)` footgun must stay flagged.
    #[test]
    fn flags_local_array_map_parse_int() {
        assert_eq!(run_on("const arr = [1, 2, 3]; arr.map(parseInt);").len(), 1);
    }

    // #4469 negative space: an array literal receiver must stay flagged.
    #[test]
    fn flags_array_literal_some() {
        assert_eq!(run_on("[1, 2].some(isOdd);").len(), 1);
    }

    // #4469 negative space: a non-namespace local object receiver must stay
    // flagged (`obj` is not an `import * as` binding).
    #[test]
    fn flags_non_namespace_local_object() {
        assert_eq!(run_on("const obj = getThing(); obj.map(transform);").len(), 1);
    }

    // #6276 repro (statelyai/xstate): `this._toTestPath` is an arrow class
    // property — auto-bound to `this` and single-parameter — so `paths.map(this._toTestPath)`
    // keeps `this` and drops the injected `index`/`array`. Must not flag.
    #[test]
    fn allows_single_param_arrow_class_property_this_method() {
        assert!(run_on(
            "class M { private _toTestPath = (statePath: string): string => statePath; getPaths(paths: string[]) { return paths.map(this._toTestPath); } }"
        )
        .is_empty());
    }

    // #6276 negative space: a normal (non-arrow) method passed as `this.handler`
    // loses `this` when detached, so the footgun applies — keep it flagged.
    #[test]
    fn flags_normal_method_this_reference() {
        assert_eq!(
            run_on(
                "class M { handler(x: number): number { return x * 2; } run(arr: number[]) { return arr.map(this.handler); } }"
            )
            .len(),
            1
        );
    }

    // #6276 negative space: a two-parameter arrow class property exposes the
    // `(element, index)` footgun via map's injected args — keep it flagged.
    #[test]
    fn flags_two_param_arrow_class_property_this_method() {
        assert_eq!(
            run_on(
                "class M { private _f = (a: number, b: number): number => a + b; run(arr: number[]) { return arr.map(this._f); } }"
            )
            .len(),
            1
        );
    }

    // #7187 repro (remult/remult `sort.ts`): `key` is a `for...in` loop variable
    // — always a `string`, never a function — so `entityDefs.fields.find(key)` is
    // a field lookup by name, not the callback footgun. Must not flag.
    #[test]
    fn allows_for_in_loop_variable_argument() {
        assert!(run_on(
            "for (const key in orderBy) { const field = entityDefs.fields.find(key); }"
        )
        .is_empty());
    }

    // #7187: a `for...in` key passed to `.map`/`.filter` is likewise exempt.
    #[test]
    fn allows_for_in_loop_variable_map_filter() {
        assert!(run_on("for (const k in obj) { arr.map(k); }").is_empty());
        assert!(run_on("for (const k in obj) { arr.filter(k); }").is_empty());
    }

    // #7187 negative space: a `for...of` element binding can legitimately hold a
    // function reference, so it must stay flagged.
    #[test]
    fn flags_for_of_element_binding_argument() {
        assert_eq!(
            run_on("for (const fn of callbacks) { arr.map(fn); }").len(),
            1
        );
    }

    // #7187 negative space: the genuine `arr.map(parseInt)` footgun is unaffected.
    #[test]
    fn flags_parse_int_alongside_for_in_fix() {
        assert_eq!(run_on("const x = arr.map(parseInt);").len(), 1);
    }

    // #7187 negative space: a `var` declared in an unbraced `for...in` body is a
    // normal binding (not the loop key), so it must stay flagged — the exemption
    // matches only the loop's `left` head, not any declaration under it.
    #[test]
    fn flags_var_in_unbraced_for_in_body() {
        assert_eq!(
            run_on("for (const k in obj) var fn = getCb(); const x = arr.map(fn);").len(),
            1
        );
    }
}
