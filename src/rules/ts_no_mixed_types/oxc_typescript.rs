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
                if has_iterator_protocol_method(&iface.body.body) {
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
                if has_iterator_protocol_method(&lit.members) {
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
