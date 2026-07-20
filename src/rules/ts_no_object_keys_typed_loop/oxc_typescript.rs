//! oxc backend — flag `Object.keys(x).forEach(k => x[k])` / `.map(...)` only
//! when `x` resolves to a type with a concrete, finite set of named keys.
//!
//! The warning ("`k` is typed `string`, so `x[k]` widens to `any` — use
//! `Object.entries` or `keyof typeof x`") is only sound when `keyof x` is a
//! finite key union (`{ a: A; b: B }`). For an `any`/`unknown` receiver `x[k]`
//! is already `any`; for a `Record<…>` / index-signature / mapped type `keyof x`
//! is `string`/`number` and `x[k]` is already correctly typed — the suggested
//! cast is a no-op. So the rule fires only when `x`'s resolved type is an
//! object-literal type, or a same-file interface / type-alias with named
//! property signatures and no index signature — the sole shape where the
//! `keyof typeof x` cast actually narrows. A receiver whose type cannot be
//! proven finite (an `any`, a `Record`, an untyped or imported binding, an
//! undeclared name) is never flagged. Names are never evidence.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, TSSignature, TSType, TSTypeName};
use oxc_semantic::{Semantic, SymbolId};
use std::sync::Arc;

/// Hop budget for following `type X = Y` alias chains, matching
/// [`crate::oxc_helpers`]'s named-type resolvers; bounds cyclic aliases.
const TYPE_ALIAS_DEPTH: u32 = 8;

pub struct Check;

/// The symbol an identifier reference resolves to, or `None` when it is
/// unresolved (an undeclared or cross-file binding).
fn reference_symbol(ident: &oxc_ast::ast::IdentifierReference, semantic: &Semantic) -> Option<SymbolId> {
    let ref_id = ident.reference_id.get()?;
    semantic.scoping().get_reference(ref_id).symbol_id()
}

/// Whether a set of type-member signatures denotes a finite named-key set: at
/// least one non-computed property signature and no index signature (an index
/// signature widens `keyof` to `string`/`number`).
fn signatures_are_finite_keyed(members: &[TSSignature]) -> bool {
    let mut has_named = false;
    for member in members {
        match member {
            TSSignature::TSIndexSignature(_) => return false,
            TSSignature::TSPropertySignature(prop) if !prop.computed => has_named = true,
            _ => {}
        }
    }
    has_named
}

/// Whether the same-file `interface`/`type` named `type_name` resolves to a
/// finite named-key set. An interface with `extends` heritage is treated
/// conservatively as non-finite (a base could contribute an index signature).
fn named_type_is_finite_keyed(type_name: &str, semantic: &Semantic, depth: u32) -> bool {
    if depth >= TYPE_ALIAS_DEPTH {
        return false;
    }
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) if decl.id.name.as_str() == type_name => {
                return decl.extends.is_empty() && signatures_are_finite_keyed(&decl.body.body);
            }
            AstKind::TSTypeAliasDeclaration(decl) if decl.id.name.as_str() == type_name => {
                return type_is_finite_keyed(&decl.type_annotation, semantic, depth + 1);
            }
            _ => {}
        }
    }
    false
}

/// Whether a type annotation denotes a concrete finite named-key set: an inline
/// object-literal type, or a same-file named `interface`/`type` that resolves to
/// one. Everything else — `any`, `unknown`, `Record<…>` and other unresolvable
/// references, unions, intersections, mapped/primitive/array types — is not.
fn type_is_finite_keyed(ty: &TSType, semantic: &Semantic, depth: u32) -> bool {
    match ty {
        TSType::TSTypeLiteral(lit) => signatures_are_finite_keyed(&lit.members),
        TSType::TSTypeReference(tref) => match &tref.type_name {
            TSTypeName::IdentifierReference(id) => {
                named_type_is_finite_keyed(id.name.as_str(), semantic, depth)
            }
            // Qualified (`A.B`) / `this` type names resolve no same-file finite key set.
            _ => false,
        },
        _ => false,
    }
}

/// Whether an initializer expression is an object literal with a finite named-key
/// set — at least one static-named property, no spread and no computed key (both
/// widen the key set beyond what is written).
fn init_is_finite_keyed_object(expr: &Expression) -> bool {
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    let mut has_named = false;
    for prop in &obj.properties {
        match prop {
            ObjectPropertyKind::ObjectProperty(p) => {
                if p.computed {
                    return false;
                }
                has_named = true;
            }
            ObjectPropertyKind::SpreadProperty(_) => return false,
        }
    }
    has_named
}

/// Whether the binding `symbol` holds a value whose type has a concrete finite
/// set of named keys — read from a `let`/`const`/`var` declarator's type
/// annotation or (absent one) its object-literal initializer, or a parameter's
/// type annotation.
fn binding_is_finite_keyed(symbol: SymbolId, semantic: &Semantic) -> bool {
    let decl_id = semantic.scoping().symbol_declaration(symbol);
    let kind = semantic.nodes().kind(decl_id);
    if let AstKind::VariableDeclarator(decl) = kind {
        return match &decl.type_annotation {
            Some(ann) => type_is_finite_keyed(&ann.type_annotation, semantic, 0),
            None => decl.init.as_ref().is_some_and(init_is_finite_keyed_object),
        };
    }
    if let AstKind::FormalParameter(param) = kind {
        return param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_finite_keyed(&ann.type_annotation, semantic, 0));
    }
    false
}

/// The `Object.keys(<ident>)` receiver identifier of `call`, when `call` is
/// exactly that shape with a single simple-identifier argument.
fn object_keys_receiver<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
    let Expression::StaticMemberExpression(callee) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &callee.object else {
        return None;
    };
    if obj.name != "Object" || callee.property.name != "keys" {
        return None;
    }
    if call.arguments.len() != 1 {
        return None;
    }
    match call.arguments[0].as_expression()? {
        Expression::Identifier(receiver) => Some(receiver),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Object.keys"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(outer) = node.kind() else {
            return;
        };
        // Callee must be `<inner>.forEach` / `<inner>.map`.
        let Expression::StaticMemberExpression(outer_callee) = &outer.callee else {
            return;
        };
        let method = match outer_callee.property.name.as_str() {
            "forEach" => ".forEach",
            "map" => ".map",
            _ => return,
        };
        // Inner receiver must be `Object.keys(<ident>)`.
        let Expression::CallExpression(inner) = &outer_callee.object else {
            return;
        };
        let Some(receiver) = object_keys_receiver(inner) else {
            return;
        };
        let Some(receiver_sym) = reference_symbol(receiver, semantic) else {
            return;
        };

        // Only fire when the receiver's resolved type has a finite named-key set.
        if !binding_is_finite_keyed(receiver_sym, semantic) {
            return;
        }

        // The callback must index the receiver (`x[k]`) — otherwise no value is
        // read through the widened key and there is nothing to warn about.
        let indexes_receiver = semantic.nodes().iter().any(|n| {
            let AstKind::ComputedMemberExpression(member) = n.kind() else {
                return false;
            };
            if member.span.start < outer.span.start || member.span.end > outer.span.end {
                return false;
            }
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            reference_symbol(obj, semantic) == Some(receiver_sym)
        });
        if !indexes_receiver {
            return;
        }

        let ident = receiver.name.as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, inner.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`Object.keys({ident}){method}(...)` types `k` as `string`, so `{ident}[k]` widens \
                 to `any`. Use `Object.entries({ident})` or cast: \
                 `(Object.keys({ident}) as Array<keyof typeof {ident}>){method}(...)`."
            ),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Fires only on a concrete finite-key receiver ---

    #[test]
    fn flags_object_literal_type_annotation() {
        let src = "const obj: { a: number; b: number } = { a: 1, b: 2 }; \
                   Object.keys(obj).forEach(k => console.log(obj[k]));";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn flags_object_literal_inference() {
        let src = "const obj = { a: 1, b: 2 }; Object.keys(obj).forEach(k => console.log(obj[k]));";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn flags_map_on_finite_keyed_binding() {
        let src = "const state: { x: number } = { x: 1 }; const r = Object.keys(state).map(k => state[k] + 1);";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn flags_interface_receiver() {
        let src = "interface Named { a: number; b: number } \
                   declare const n: Named; \
                   Object.keys(n).forEach(k => use(n[k]));";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn flags_type_alias_receiver() {
        let src = "type Named = { a: number; b: number }; \
                   declare const n: Named; \
                   Object.keys(n).map(k => n[k]);";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // --- Regression: FP receivers whose `keyof` is `string`/`number` (#7881) ---

    // mikro-orm entity/validators.ts:60 — `params: any`.
    #[test]
    fn allows_any_parameter_receiver() {
        let src = "function validateParams(params: any) { \
                   Object.keys(params).forEach(k => use(params[k])); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Minimal reproducer, line 1 — `declare const anyObj: any`.
    #[test]
    fn allows_declared_any_receiver() {
        let src = "declare const anyObj: any; Object.keys(anyObj).forEach(k => use(anyObj[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Minimal reproducer, line 2 — `declare const rec: Record<string, number>`.
    #[test]
    fn allows_record_receiver() {
        let src = "declare const rec: Record<string, number>; \
                   Object.keys(rec).forEach(k => use(rec[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_inline_index_signature_receiver() {
        let src = "const dict: { [k: string]: number } = {}; \
                   Object.keys(dict).forEach(k => use(dict[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_type_alias_index_signature_receiver() {
        let src = "type Dict = { [k: string]: number }; \
                   declare const d: Dict; \
                   Object.keys(d).forEach(k => use(d[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A mapped type's `keyof` is `string`/`number` — the issue lists it as an FP class.
    #[test]
    fn allows_mapped_type_receiver() {
        let src = "type Mapped = { [k in string]: number }; \
                   declare const m: Mapped; \
                   Object.keys(m).forEach(k => use(m[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // An interface extending an index-signature base has `keyof` = `string`; the
    // conservative `extends`-heritage skip avoids smuggling that index signature in.
    #[test]
    fn allows_interface_with_extends_heritage() {
        let src = "interface Base { [k: string]: number } \
                   interface Ext extends Base { a: number } \
                   declare const e: Ext; \
                   Object.keys(e).forEach(k => use(e[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // mikro-orm utils/QueryHelper.ts:382 — `options: FilterOptions | undefined`,
    // a union (never a concrete finite key set).
    #[test]
    fn allows_union_parameter_receiver() {
        let src = "function copy(options: FilterOptions | undefined) { \
                   const opts: Record<string, unknown> = {}; \
                   Object.keys(options).forEach(f => (opts[f] = options[f])); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A name is never evidence: an undeclared receiver is unresolvable.
    #[test]
    fn allows_undeclared_receiver() {
        let src = "Object.keys(obj).forEach(k => console.log(obj[k]));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // --- Shape guards preserved from the text implementation ---

    #[test]
    fn allows_object_entries() {
        let src = "const obj = { a: 1 }; Object.entries(obj).forEach(([k, v]) => console.log(v));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_finite_keyed_without_index() {
        let src = "const obj = { a: 1 }; Object.keys(obj).forEach(k => log(k));";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_for_of_entries() {
        let src = "const obj = { a: 1 }; for (const [k, v] of Object.entries(obj)) { log(k, v); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
