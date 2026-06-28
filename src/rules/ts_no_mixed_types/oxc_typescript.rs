//! ts-no-mixed-types OxcCheck backend.
//!
//! Flag interfaces / type aliases (with object-type value) that mix property
//! signatures with method signatures.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey, TSMethodSignature, TSSignature};
use std::sync::Arc;

pub struct Check;

fn scan_signatures(sigs: &oxc_allocator::Vec<'_, TSSignature>) -> (bool, bool) {
    let mut has_property = false;
    let mut has_method = false;
    for sig in sigs {
        match sig {
            TSSignature::TSPropertySignature(_) => has_property = true,
            TSSignature::TSMethodSignature(_) => has_method = true,
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
                if has_iterator_protocol_method(&iface.body.body)
                    || has_call_signature(&iface.body.body)
                {
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
                    severity: Severity::Warning,
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
                if has_iterator_protocol_method(&lit.members)
                    || has_call_signature(&lit.members)
                {
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
}
