//! ts-no-mixed-types OxcCheck backend.
//!
//! Flag interfaces / type aliases (with object-type value) that mix property
//! signatures with method signatures.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, PropertyKey, TSMethodSignature, TSMethodSignatureKind, TSSignature,
};
use std::borrow::Cow;
use std::sync::Arc;

pub struct Check;

fn scan_signatures(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> (bool, bool) {
    let mut has_property = false;
    let mut has_method = false;
    for sig in sigs {
        match sig {
            TSSignature::TSPropertySignature(_) => has_property = true,
            // Only true methods (`foo(): T`) count. Get/Set accessor signatures
            // (`get x(): T` / `set x(v)`) are also `TSMethodSignature` nodes, but
            // they encode distinct read/write contracts that no single property
            // signature can express, so they aren't part of the property-vs-method
            // mix this rule targets.
            TSSignature::TSMethodSignature(method)
                if method.kind == TSMethodSignatureKind::Method =>
            {
                has_method = true
            }
            _ => {}
        }
    }
    (has_property, has_method)
}

/// A method signature implements the JS iterator/iterable protocol when its
/// key is `next`, `return`, `throw`, or a computed `[Symbol.iterator]` /
/// `[Symbol.asyncIterator]`. These members cannot use property-signature
/// syntax without breaking the protocol, so an interface that mixes them with
/// property signatures is doing so by necessity, not by accident.
fn is_iterator_protocol_method(method: &TSMethodSignature<'_>) -> bool {
    match &method.key {
        PropertyKey::StaticIdentifier(id) => matches!(id.name.as_str(), "next" | "return" | "throw"),
        PropertyKey::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::Identifier(obj) if obj.name == "Symbol")
                && matches!(member.property.name.as_str(), "iterator" | "asyncIterator")
        }
        _ => false,
    }
}

fn has_iterator_protocol_method(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> bool {
    sigs.iter().any(|sig| match sig {
        TSSignature::TSMethodSignature(method) => is_iterator_protocol_method(method),
        _ => false,
    })
}

/// An anonymous call signature (`(...): T`) or construct signature
/// (`new (...): T`) makes the interface a hybrid callable/constructable
/// function-object (jQuery `$`-style). Its non-call members describe slots
/// attached to the function value, where mixing property signatures (function
/// values) with method signatures (true methods) is an intentional design, not
/// the plain data-model inconsistency this rule targets.
fn has_call_signature(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> bool {
    sigs.iter().any(|sig| {
        matches!(
            sig,
            TSSignature::TSCallSignatureDeclaration(_)
                | TSSignature::TSConstructSignatureDeclaration(_)
        )
    })
}

/// The static name of a true method signature (`foo(): T`). A literal key
/// (`"foo"()`, `1()`) names its member as an identifier key does. Get/Set
/// accessor signatures repeat their counterpart's name without being overloads,
/// so they carry no name here, and neither do keys computed from an expression
/// (`[Symbol.iterator]`).
fn method_name<'a>(sig: &TSSignature<'a>) -> Option<Cow<'a, str>> {
    let TSSignature::TSMethodSignature(method) = sig else { return None };
    if method.kind != TSMethodSignatureKind::Method {
        return None;
    }
    method.key.static_name()
}

/// A member is overloaded when its name carries two or more method signatures.
/// The property form of an overload set is a nested call-signature literal
/// (`parse: { (a: string): T; (a: number): T }`) — a different declaration
/// shape, not a signature-style switch — so an overloaded member leaves the
/// author no like-for-like all-properties alternative.
fn has_overloaded_method_signatures(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> bool {
    sigs.iter().enumerate().any(|(index, sig)| {
        let Some(name) = method_name(sig) else { return false };
        sigs[index + 1..].iter().filter_map(method_name).any(|later| later == name)
    })
}

/// The property/method mix carries no signal when the method syntax is not a
/// free stylistic choice: iterator-protocol members and overloaded members
/// require it, and a call signature marks a hybrid function-object whose mixed
/// members are an intentional design. Each exemption covers the whole
/// declaration: `ts-method-signature-style` asks for the reverse rewrite, so
/// turning the remaining properties into methods is never the way out either.
fn is_exempt_mix(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> bool {
    has_iterator_protocol_method(sigs)
        || has_call_signature(sigs)
        || has_overloaded_method_signatures(sigs)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(iface) => {
                let (has_prop, has_method) = scan_signatures(&iface.body.body);
                if !(has_prop && has_method) {
                    return;
                }
                if is_exempt_mix(&iface.body.body) {
                    return;
                }
                let name = iface.id.name.as_str();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, iface.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` mixes property signatures with method signatures \u{2014} use \
                         consistent signatures: either all properties or all methods."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::TSTypeAliasDeclaration(alias) => {
                let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &alias.type_annotation else {
                    return;
                };
                let (has_prop, has_method) = scan_signatures(&lit.members);
                if !(has_prop && has_method) {
                    return;
                }
                if is_exempt_mix(&lit.members) {
                    return;
                }
                let name = alias.id.name.as_str();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, alias.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` mixes property signatures with method signatures \u{2014} use \
                         consistent signatures: either all properties or all methods."
                    ),
                    severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_mixed_interface() {
        let d = run_on("interface User { name: string; greet(): void; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("User"));
    }

    #[test]
    fn flags_mixed_type_alias() {
        let d = run_on("type User = { name: string; greet(): void; };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("User"));
    }

    #[test]
    fn allows_paged_async_iterable_iterator() {
        // From azure-sdk-for-js: protocol methods (`next`, `[Symbol.asyncIterator]`)
        // must use method syntax, while `byPage` is intentionally a property
        // signature for stricter variance. The mix is unavoidable, not a smell.
        let d = run_on(
            r#"
interface PagedAsyncIterableIterator<TElement, TPage, TPageSettings> {
    next(): Promise<IteratorResult<TElement>>;
    [Symbol.asyncIterator](): PagedAsyncIterableIterator<TElement, TPage, TPageSettings>;
    byPage: (settings?: TPageSettings) => AsyncIterableIterator<ContinuablePage<TElement, TPage>>;
}
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_sync_iterator_with_property() {
        let d = run_on(
            r#"
interface CustomIterator<T> {
    [Symbol.iterator](): CustomIterator<T>;
    next(): IteratorResult<T>;
    return(): IteratorResult<T>;
    throw(): IteratorResult<T>;
    map: (fn: (v: T) => T) => CustomIterator<T>;
}
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_callable_hybrid_interface() {
        // From unjs/defu (src/types.ts): `DefuInstance` is a callable interface
        // (anonymous call signature). The mix of property signatures (`fn`,
        // `arrayFn`) and a method signature (`extend`) describes slots attached
        // to the function value, an intentional function-object design.
        let d = run_on(
            r#"
export interface DefuInstance {
    <Source extends Input, Defaults extends Array<Input | IgnoredInput>>(
        source: Source | IgnoredInput,
        ...defaults: Defaults
    ): Defu<Source, Defaults>;
    fn: DefuFn;
    arrayFn: DefuFn;
    extend(merger?: Merger): DefuFn;
}
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_constructable_hybrid_interface() {
        // A construct signature (`new (...)`) likewise marks a hybrid
        // function-object; mixing property and method members is intentional.
        let d = run_on(
            r#"
interface WidgetFactory {
    new (name: string): Widget;
    defaults: WidgetOptions;
    create(name: string): Widget;
}
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_callable_hybrid_type_alias() {
        // Parity with the interface branch: a callable object-type alias is a
        // hybrid function-object, so its property/method mix is intentional.
        let d = run_on(
            r#"
type DefuType = {
    <Source>(source: Source): Source;
    fn: DefuFn;
    extend(merger?: Merger): DefuFn;
};
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_plain_interface_without_call_signature() {
        // Negative control: a plain data-model interface mixing a property with
        // a method but carrying NO call/construct signature is still flagged.
        let d = run_on("interface X { a: () => void; b(): void; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("X"));
    }

    #[test]
    fn still_flags_mixed_interface_without_protocol_method() {
        // A method named `next` is the protocol exemption; an unrelated method
        // mixed with a property is still a genuine smell.
        let d = run_on(
            r#"
interface UserConfig {
    name: string;
    validate(): boolean;
}
"#,
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("UserConfig"));
    }

    #[test]
    fn allows_property_with_get_set_accessor_pair() {
        // Regression rbaumier/comply#7354 (strapi/strapi, index.ts) — a data
        // property alongside a get/set accessor pair is not a property-vs-method
        // mix. The getter returns `ParsedQuery` while the setter takes `any`, so
        // no single property signature can express both; the accessors aren't
        // methods and the interface is not flagged.
        let d = run_on(
            r#"
interface BaseRequest {
    _querycache?: ParsedQuery;
    get query(): ParsedQuery;
    set query(obj: any);
}
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_property_with_getter_only() {
        // A data property plus a lone getter accessor signature: the getter is
        // not a method signature, so there is no property/method mix to flag.
        let d = run_on("interface OnlyAccessors { data: string; get x(): number; }");
        assert!(d.is_empty());
    }

    /// Shape of colinhacks/zod `$ZodFunction` (packages/zod/src/v4/core/schemas.ts),
    /// with `input_signatures` holding either the three `input` overloads or a
    /// single `input` signature.
    fn zod_function_interface(input_signatures: &str) -> String {
        format!(
            r#"
export interface $ZodFunction<
    Args extends $ZodFunctionIn = $ZodFunctionIn,
    Returns extends $ZodFunctionOut = $ZodFunctionOut,
> extends $ZodType<any, any, $ZodFunctionInternals<Args, Returns>> {{
    _def: $ZodFunctionDef<Args, Returns>;
    _input: $InferInnerFunctionType<Args, Returns>;
    _output: $InferOuterFunctionType<Args, Returns>;

    implement<F extends $InferInnerFunctionType<Args, Returns>>(func: F): F;

{input_signatures}

    output<NewReturns extends $ZodType>(output: NewReturns): $ZodFunction<Args, NewReturns>;
}}
"#
        )
    }

    #[test]
    fn allows_interface_with_overloaded_method() {
        // Regression rbaumier/comply#6845 (colinhacks/zod) — `input` carries three
        // overload signatures, whose property form would be a nested call-signature
        // literal, so the mix with the `_def` / `_input` / `_output` data slots is
        // forced.
        let d = run_on(&zod_function_interface(
            r#"
    input<const Items extends util.TupleItems, const Rest extends $ZodFunctionOut = $ZodFunctionOut>(
        args: Items,
        rest?: Rest
    ): $ZodFunction<$ZodTuple<Items, Rest>, Returns>;
    input<NewArgs extends $ZodFunctionIn>(args: NewArgs): $ZodFunction<NewArgs, Returns>;
    input(...args: any[]): $ZodFunction<any, Returns>;
"#,
        ));
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_same_interface_without_overloads() {
        // Non-vacuity control for `allows_interface_with_overloaded_method`: the
        // same source with `input` collapsed to one signature parses and is
        // flagged, so the exemption comes from the overload, not from the shape
        // of the snippet.
        let d = run_on(&zod_function_interface(
            "    input<NewArgs extends $ZodFunctionIn>(args: NewArgs): $ZodFunction<NewArgs, Returns>;",
        ));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("$ZodFunction"));
    }

    #[test]
    fn allows_type_alias_with_overloaded_method() {
        // Parity with the interface branch: a type literal can carry overloads too.
        let d = run_on(
            r#"
type Parser = {
    source: string;
    parse(input: string): Node;
    parse(input: Buffer, encoding: string): Node;
};
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_interface_with_literal_key_overload() {
        // A computed literal key (`["parse"]`) names the same member as an
        // identifier key would, so repeating it is a real overload.
        let d = run_on(
            r#"
interface Codec {
    data: string;
    ["parse"](input: string): Node;
    ["parse"](input: number, radix: number): Node;
}
"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_mixed_interface_with_get_set_pair() {
        // A get/set pair repeats a name without being an overload, so it must not
        // trip the overload exemption: the singleton `refresh` method mixed with
        // the `data` property is still flagged.
        let d = run_on(
            r#"
interface Store {
    data: string;
    get size(): number;
    set size(v: number);
    refresh(): void;
}
"#,
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Store"));
    }

    #[test]
    fn still_flags_property_with_real_method() {
        // Negative control: a data property mixed with a genuine callable method
        // (`register` has `kind == Method`) is still flagged.
        let d = run_on("interface AsyncHook { handlers: Handler[]; register(h: Handler): void; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("AsyncHook"));
    }
}
