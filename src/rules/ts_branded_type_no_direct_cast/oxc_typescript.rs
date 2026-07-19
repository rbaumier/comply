//! ts-branded-type-no-direct-cast OXC backend — forbid `as BrandedType`
//! outside a function whose declared return type is that brand (its factory).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, name_is_generic_type_param_in_scope};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, PropertyKey, TSSignature, TSType, TSTypeName, TSTypeOperatorOperator,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_branded_name(name: &str) -> bool {
    name.contains("Brand")
        || name.ends_with("Id")
        || name.ends_with("Uuid")
        || name.ends_with("UUID")
        || name.ends_with("Token")
        || name.ends_with("Hash")
}

/// True when the cast operand (peeling parentheses) is a `Symbol(...)` or
/// `Symbol.<method>(...)` call — e.g. `Symbol.for(key)`. Casting the result to a
/// `unique symbol` TypeId brand is the canonical, and only possible, construction
/// of that symbol marker: the brand has exactly one inhabitant — the symbol itself
/// — so there is nothing to validate. The signal is the built-in `Symbol`
/// constructor call, read locally without type resolution; an arbitrary data-brand
/// cast (`'' as UserId`, `raw as EntityId`) never routes through `Symbol`.
fn operand_is_symbol_construction(operand: &Expression) -> bool {
    match operand {
        Expression::ParenthesizedExpression(p) => operand_is_symbol_construction(&p.expression),
        Expression::CallExpression(call) => match &call.callee {
            // `Symbol(key)` — the global constructor.
            Expression::Identifier(id) => id.name.as_str() == "Symbol",
            // `Symbol.for(key)` and other `Symbol.<method>(...)` calls.
            Expression::StaticMemberExpression(member) => {
                matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "Symbol")
            }
            _ => false,
        },
        _ => false,
    }
}

/// Generic helpers from the common branded-type libraries whose instantiation
/// (`Brand<string, 'UserId'>`) produces a nominal type.
fn is_brand_helper_name(name: &str) -> bool {
    matches!(
        name,
        "Brand" | "Branded" | "Tagged" | "Opaque" | "Flavor" | "Flavored" | "Nominal" | "WithBrand"
    )
}

/// Object-member keys that act as a phantom brand marker on the object operand
/// of a branding intersection (`string & { readonly __brand: 'UserId' }`).
fn is_brand_marker_key(key: &str) -> bool {
    matches!(key, "__brand" | "_brand" | "brand" | "__tag" | "_tag" | "tag")
}

/// True when `ty` is structurally a branded/nominal type: an intersection that
/// adds a phantom brand marker to a base type, or a reference to a known brand
/// helper generic. Plain unions, primitives, and ordinary aliases are not.
fn is_structurally_branded(ty: &TSType) -> bool {
    match ty {
        TSType::TSParenthesizedType(p) => is_structurally_branded(&p.type_annotation),
        TSType::TSTypeReference(r) => {
            r.type_arguments.is_some()
                && matches!(&r.type_name, TSTypeName::IdentifierReference(id) if is_brand_helper_name(id.name.as_str()))
        }
        TSType::TSIntersectionType(intersection) => {
            intersection.types.iter().any(is_brand_marker_operand)
        }
        _ => false,
    }
}

/// True when `ty` is a generic instantiation (`RoutesById<TRouteTree>`) whose base
/// is not a brand helper — a container/mapped/record type, not a nominal brand.
/// Mirrors the brand-helper gate in `is_structurally_branded`: a generic
/// instantiation is branded only when its base passes `is_brand_helper_name`
/// (`Brand<string, 'user'>`); any other `Foo<Bar>` is an ordinary generic type.
/// The base is compared on its trailing segment, so a namespace-qualified helper
/// (`B.Brand<…>`) still counts as a brand.
fn is_non_brand_generic_instantiation(ty: &TSType) -> bool {
    match ty {
        TSType::TSParenthesizedType(p) => is_non_brand_generic_instantiation(&p.type_annotation),
        TSType::TSTypeReference(r) if r.type_arguments.is_some() => {
            !type_reference_base_name(ty).is_some_and(is_brand_helper_name)
        }
        _ => false,
    }
}

/// True when an operand of an intersection is a phantom brand marker: an object
/// literal carrying a `__brand`/`__tag`/… property, or a `unique symbol`.
fn is_brand_marker_operand(ty: &TSType) -> bool {
    match ty {
        TSType::TSParenthesizedType(p) => is_brand_marker_operand(&p.type_annotation),
        TSType::TSTypeOperatorType(op) => op.operator == TSTypeOperatorOperator::Unique,
        TSType::TSTypeLiteral(lit) => lit.members.iter().any(|member| {
            let TSSignature::TSPropertySignature(prop) = member else {
                return false;
            };
            match &prop.key {
                PropertyKey::StaticIdentifier(id) => is_brand_marker_key(id.name.as_str()),
                PropertyKey::StringLiteral(s) => is_brand_marker_key(s.value.as_str()),
                _ => false,
            }
        }),
        TSType::TSTypeReference(r) => {
            matches!(&r.type_name, TSTypeName::IdentifierReference(id) if is_brand_helper_name(id.name.as_str()))
        }
        _ => false,
    }
}

/// Looks up a local `type <name> = …` alias declared in the same file and
/// reports whether its definition is structurally branded.
///
/// Returns `None` when no such alias exists in the file — the target is then
/// imported or built-in, so the cast site has no structure to inspect and the
/// caller falls back to the name heuristic.
fn local_alias_is_branded<'a>(
    name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<bool> {
    // A namespace-qualified target (`Schemas.ColorSchemeId`) is declared under
    // its bare identifier; compare against the trailing segment.
    let bare = name.rsplit('.').next().unwrap_or(name);
    let mut found = None;
    for node in semantic.nodes().iter() {
        if let AstKind::TSTypeAliasDeclaration(alias) = node.kind()
            && alias.id.name.as_str() == bare
        {
            // A later, structurally-branded declaration of the same name wins
            // (e.g. an alias re-exporting an imported brand); keep scanning.
            let branded = is_structurally_branded(&alias.type_annotation);
            found = Some(found.unwrap_or(false) || branded);
        }
    }
    found
}

/// Returns the definition of a local `type <name> = …` alias declared in the
/// same file, so a wrapper return type (`TrackedEnvelope`) can be resolved to
/// the type tree it stands for. `None` when the name is imported or built-in and
/// thus has no in-file structure to inspect. First match wins: TypeScript forbids
/// two same-named aliases in one scope, so a single definition is authoritative.
fn local_alias_definition<'a>(
    name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a TSType<'a>> {
    // A namespace-qualified target (`Schemas.TrackedEnvelope`) is declared under
    // its bare trailing segment, as `local_alias_is_branded` also compares.
    let bare = name.rsplit('.').next().unwrap_or(name);
    semantic.nodes().iter().find_map(|node| {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return None;
        };
        (alias.id.name.as_str() == bare).then_some(&alias.type_annotation)
    })
}

/// Base name of a type reference (`Brand<…>` → `Brand`), or `None` for any
/// other type form. Used to compare a return-type operand against the brand the
/// cast targets.
fn type_reference_base_name<'a>(ty: &TSType<'a>) -> Option<&'a str> {
    match ty {
        TSType::TSParenthesizedType(p) => type_reference_base_name(&p.type_annotation),
        TSType::TSTypeReference(r) => match &r.type_name {
            TSTypeName::IdentifierReference(id) => Some(id.name.as_str()),
            TSTypeName::QualifiedName(q) => Some(q.right.name.as_str()),
            TSTypeName::ThisExpression(_) => None,
        },
        _ => None,
    }
}

/// Bounds alias resolution so a cyclic alias chain (`type A = B; type B = A`)
/// cannot recurse forever; genuine return-type trees nest far shallower.
const MAX_ALIAS_DEPTH: u32 = 16;

/// True when a type reference named `brand` appears anywhere in `ty`'s resolved
/// tree: directly, inside tuple elements, union/intersection operands, array
/// elements, generic type arguments, or a local alias the type refers to (looked
/// up via `local_alias_definition`). This recognises a factory whose declared
/// return type *wraps* the brand — `TrackedEnvelope<T> = [TrackedId, T, …]` — not
/// only one returning the brand directly (`Brand`, `Brand | undefined`). `depth`
/// caps alias-chain recursion. Object-property wrapping (`{ id: Brand }`) is
/// intentionally not walked: a narrow exemption avoids silencing genuine casts in
/// functions that merely return a struct mentioning the brand.
fn brand_in_resolved_type<'a>(
    ty: &TSType<'a>,
    brand: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    depth: u32,
) -> bool {
    if depth == 0 {
        return false;
    }
    match ty {
        TSType::TSTypeReference(r) => {
            let base = type_reference_base_name(ty);
            if base == Some(brand) {
                return true;
            }
            let arg_has_brand = r.type_arguments.as_ref().is_some_and(|args| {
                args.params
                    .iter()
                    .any(|arg| brand_in_resolved_type(arg, brand, semantic, depth - 1))
            });
            arg_has_brand
                || base
                    .and_then(|name| local_alias_definition(name, semantic))
                    .is_some_and(|def| brand_in_resolved_type(def, brand, semantic, depth - 1))
        }
        TSType::TSParenthesizedType(p) => {
            brand_in_resolved_type(&p.type_annotation, brand, semantic, depth - 1)
        }
        TSType::TSArrayType(arr) => {
            brand_in_resolved_type(&arr.element_type, brand, semantic, depth - 1)
        }
        TSType::TSTypeOperatorType(op) => {
            brand_in_resolved_type(&op.type_annotation, brand, semantic, depth - 1)
        }
        TSType::TSUnionType(u) => u
            .types
            .iter()
            .any(|t| brand_in_resolved_type(t, brand, semantic, depth - 1)),
        TSType::TSIntersectionType(i) => i
            .types
            .iter()
            .any(|t| brand_in_resolved_type(t, brand, semantic, depth - 1)),
        TSType::TSTupleType(tuple) => tuple
            .element_types
            .iter()
            .any(|el| tuple_element_contains_brand(el, brand, semantic, depth - 1)),
        TSType::TSNamedTupleMember(member) => {
            tuple_element_contains_brand(&member.element_type, brand, semantic, depth - 1)
        }
        _ => false,
    }
}

/// Bridges a `TSTupleElement` (possibly optional/rest-wrapped) back into
/// `brand_in_resolved_type`, mirroring tuple handling in other type walks.
fn tuple_element_contains_brand<'a>(
    el: &oxc_ast::ast::TSTupleElement<'a>,
    brand: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
    depth: u32,
) -> bool {
    use oxc_ast::ast::TSTupleElement;
    match el {
        TSTupleElement::TSOptionalType(opt) => {
            brand_in_resolved_type(&opt.type_annotation, brand, semantic, depth)
        }
        TSTupleElement::TSRestType(rest) => {
            brand_in_resolved_type(&rest.type_annotation, brand, semantic, depth)
        }
        other => other
            .as_ts_type()
            .is_some_and(|inner| brand_in_resolved_type(inner, brand, semantic, depth)),
    }
}

/// True when an enclosing function/arrow's declared return type contains `brand`
/// — either directly, or wrapped inside a tuple/intersection/generic resolved
/// through local aliases. Such a function IS the brand's factory: it takes
/// ownership of the raw value and stamps it with the brand, so the `as Brand`
/// cast on the return path is the single sanctioned place the boundary is crossed.
fn is_inside_brand_factory<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    brand: &str,
) -> bool {
    // A namespace-qualified target (`Schemas.NodeId`) is declared and referenced
    // under its bare trailing segment; compare on that, as `local_alias_is_branded` does.
    let bare = brand.rsplit('.').next().unwrap_or(brand);
    semantic.nodes().ancestors(node.id()).any(|ancestor| {
        let return_type = match ancestor.kind() {
            AstKind::Function(f) => f.return_type.as_deref(),
            AstKind::ArrowFunctionExpression(a) => a.return_type.as_deref(),
            _ => None,
        };
        return_type.is_some_and(|ann| {
            brand_in_resolved_type(&ann.type_annotation, bare, semantic, MAX_ALIAS_DEPTH)
        })
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        let type_span = as_expr.type_annotation.span();
        let type_text = &ctx.source[type_span.start as usize..type_span.end as usize];
        let base_name = type_text.split('<').next().unwrap_or(type_text).trim();
        if !is_branded_name(base_name) {
            return;
        }

        // A symbol value cast to a symbol-typed brand is the canonical
        // `unique symbol` TypeId construction (`Symbol.for(key) as X.TypeId`): the
        // brand has exactly one inhabitant — the symbol itself — so the cast IS the
        // construction, not a validation bypass.
        if operand_is_symbol_construction(&as_expr.expression) {
            return;
        }

        // The target resolves to a generic type parameter of an enclosing scope
        // (`class BaseRoute<TId extends string> { … id as TId }`). A type parameter
        // is a per-instantiation placeholder, not a nominal brand — there is no
        // validator to route through — so casting to it is idiomatic narrowing.
        if name_is_generic_type_param_in_scope(base_name, node.id(), semantic) {
            return;
        }

        // The name heuristic catches `*Id`/`*Token`/… but plain string-union
        // aliases share that naming (`type ColorSchemeId = 'a' | 'b'`) without
        // being nominal. When the target type is defined in this file, trust its
        // structure: only an intersection that adds a brand marker (or a brand
        // helper instantiation) is genuinely branded. Imported/built-in targets
        // have no local definition to inspect, so they keep the name heuristic.
        let local_branded = local_alias_is_branded(base_name, semantic);
        if local_branded == Some(false) {
            return;
        }

        // A target with no local definition whose cast site is a generic
        // instantiation of a non-brand-helper base (`routesById as
        // RoutesById<TRouteTree>`) is a container/mapped/record type, not a nominal
        // brand — only a brand-helper instantiation carries a brand, as
        // `is_structurally_branded` gates. A locally-defined target is left to
        // `local_alias_is_branded` above so a local branded generic alias still fires.
        if local_branded.is_none() && is_non_brand_generic_instantiation(&as_expr.type_annotation) {
            return;
        }

        if is_inside_brand_factory(node, semantic, base_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Direct cast to branded type `{base_name}`; route through a validator/constructor function."),
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
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn skips_direct_cast_in_test_file() {
        // Issue #1350 — pnpm test fixtures cast fake strings to branded types
        // (`PkgIdWithPatchHash`, `PkgResolutionId`) to avoid running production
        // validation. The central skip_in_test_dir gate exempts the whole file.
        let src = r#"const fooPkg = {
  name: 'foo',
  pkgIdWithPatchHash: 'foo/1.0.0' as PkgIdWithPatchHash,
  id: '' as PkgResolutionId,
};"#;
        let diags = run_rule_gated(
            &Check,
            src,
            "installing/deps-resolver/test/resolvePeers.ts",
        );
        assert!(diags.is_empty(), "branded cast in a test file must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_direct_cast_in_production_source() {
        // Negative space: the same pattern in shippable source is the
        // type-safety anti-pattern the rule targets and must still fire.
        let src = "const id = '' as PkgResolutionId;";
        let diags = run_rule_gated(&Check, src, "src/installing/deps-resolver/resolvePeers.ts");
        assert_eq!(diags.len(), 1, "branded cast in production source must fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_to_local_string_union_alias() {
        // Issue #4752 — `ColorSchemeId` is a plain string-literal-union alias,
        // not a nominal brand. Its name ends with `Id`, but the local definition
        // is a union, so casting to it is legitimate narrowing, not a brand bypass.
        let src = r#"type ColorSchemeId = 'nivo' | 'accent' | 'dark2' | 'auto';
export const getColorSchemeType = (scheme: ColorSchemeId | ColorInterpolatorId) => {
    if (isCategoricalColorScheme(scheme as ColorSchemeId)) {
        return 'Categorical';
    }
    return isDivergingColorScheme(scheme as ColorSchemeId) ? 'Diverging' : 'Other';
};"#;
        let diags = run_rule_gated(&Check, src, "src/components/colorSchemeSelect.tsx");
        assert!(diags.is_empty(), "cast to a local string-union alias must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_to_local_branded_intersection_alias() {
        // Negative space: a genuinely branded alias (`string & { __brand }`)
        // defined in the same file is a nominal type — direct casts to it bypass
        // the validator and must still fire.
        let src = r#"type UserId = string & { readonly __brand: 'UserId' };
const id = rawValue as UserId;"#;
        let diags = run_rule_gated(&Check, src, "src/user/id.ts");
        assert_eq!(diags.len(), 1, "cast to a local branded intersection alias must fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_to_local_brand_helper_alias() {
        // A `Brand<string, 'OrderId'>` helper instantiation is nominal too.
        let src = r#"type OrderId = Brand<string, 'OrderId'>;
const id = raw as OrderId;"#;
        let diags = run_rule_gated(&Check, src, "src/order/id.ts");
        assert_eq!(diags.len(), 1, "cast to a local brand-helper alias must fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_to_local_plain_string_alias() {
        // A bare-`string` alias named `*Token` is not nominal — no brand marker.
        let src = r#"type SessionToken = string;
const t = raw as SessionToken;"#;
        let diags = run_rule_gated(&Check, src, "src/session/token.ts");
        assert!(diags.is_empty(), "cast to a plain string alias must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_in_brand_factory_function_declaration() {
        // Issue #5696 — the factory whose declared return type IS the brand is
        // the sanctioned place the cast happens; its name need not match any
        // validator-prefix convention (`nextNodeId`, not `makeNodeId`).
        let src = r#"type Brand<K, T> = K & { __brand: T };
export type NodeId = Brand<string | number, 'nodeId'>;
let nodeIdCounter = 0;
export function nextNodeId (): NodeId {
  return ++nodeIdCounter as NodeId;
}"#;
        let diags = run_rule_gated(&Check, src, "src/deps-resolver/nextNodeId.ts");
        assert!(diags.is_empty(), "cast on the return path of the brand's factory must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_in_brand_factory_arrow_function() {
        // Same exemption for an arrow factory with an explicit brand return type.
        let src = r#"type UserId = string & { readonly __brand: 'UserId' };
export const makeUserId = (raw: string): UserId => raw as UserId;"#;
        let diags = run_rule_gated(&Check, src, "src/user/id.ts");
        assert!(diags.is_empty(), "cast in an arrow brand factory must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_in_brand_factory_with_union_return_type() {
        // A factory may legitimately return `Brand | undefined`; the cast on the
        // success path is still inside the brand's own constructor.
        let src = r#"type UserId = string & { readonly __brand: 'UserId' };
function parseUserId(raw: string): UserId | undefined {
  if (raw.length === 0) return undefined;
  return raw as UserId;
}"#;
        let diags = run_rule_gated(&Check, src, "src/user/id.ts");
        assert!(diags.is_empty(), "cast in a factory returning `Brand | undefined` must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_in_factory_returning_namespace_qualified_brand() {
        // A namespace-qualified return type (`Schemas.NodeId`) resolves to the
        // bare brand; its factory is exempt like any other.
        let src = r#"namespace Schemas {
    export type NodeId = string & { readonly __brand: 'NodeId' };
}
function makeNodeId(raw: string): Schemas.NodeId {
  return raw as Schemas.NodeId;
}"#;
        let diags = run_rule_gated(&Check, src, "src/node/id.ts");
        assert!(diags.is_empty(), "cast in a factory returning a qualified brand must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_in_function_returning_other_type() {
        // Gate: a cast to the brand inside a function whose declared return type
        // is NOT the brand bypasses the factory and must still fire, regardless
        // of a validator-shaped name.
        let src = r#"type UserId = string & { readonly __brand: 'UserId' };
function makeUserId(raw: string): string {
  const id = raw as UserId;
  return String(id);
}"#;
        let diags = run_rule_gated(&Check, src, "src/user/id.ts");
        assert_eq!(diags.len(), 1, "cast in a function not returning the brand must fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_in_brand_factory_with_wrapper_alias_return_type() {
        // Issue #6848 — `tracked()` is the sole factory for `TrackedId`. Its
        // return type is a local alias `TrackedEnvelope<T> = [TrackedId, T, …]`
        // that wraps the brand in a tuple, so the brand never appears at the top
        // of the return annotation. Resolving the alias finds `TrackedId` inside
        // the tuple, marking the function the brand's factory; the cast is exempt.
        let src = r#"const trackedSymbol = Symbol();
type TrackedId = string & { __brand: 'TrackedId' };
export type TrackedEnvelope<TData> = [TrackedId, TData, typeof trackedSymbol];
export function tracked<TData>(id: string, data: TData): TrackedEnvelope<TData> {
  if (id === '') {
    throw new Error('`id` must not be an empty string');
  }
  return [id as TrackedId, data, trackedSymbol];
}"#;
        let diags = run_rule_gated(&Check, src, "src/stream/tracked.ts");
        assert!(diags.is_empty(), "cast in a factory whose return type wraps the brand in a tuple alias must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_when_wrapper_alias_return_type_lacks_the_brand() {
        // Gate: resolving the wrapper alias is brand-specific. A function whose
        // tuple-alias return type does NOT contain `TrackedId` is not that brand's
        // factory, so a direct `as TrackedId` cast inside it still bypasses the
        // validator and must fire.
        let src = r#"type TrackedId = string & { __brand: 'TrackedId' };
type OtherEnvelope = [string, number];
function build(raw: string): OtherEnvelope {
  const id = raw as TrackedId;
  return [String(id), 0];
}"#;
        let diags = run_rule_gated(&Check, src, "src/stream/other.ts");
        assert_eq!(diags.len(), 1, "cast to a brand absent from the wrapper return type must still fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_in_factory_returning_brand_in_generic_argument() {
        // The brand may sit in a generic type argument of an imported wrapper
        // (`Promise<UserId>`) with no local alias to resolve; the return-type walk
        // must descend into type arguments to recognise the factory.
        let src = r#"type UserId = string & { readonly __brand: 'UserId' };
async function loadUserId(raw: string): Promise<UserId> {
  return raw as UserId;
}"#;
        let diags = run_rule_gated(&Check, src, "src/user/id.ts");
        assert!(diags.is_empty(), "cast in a factory returning the brand inside a generic argument must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_when_return_type_is_cyclic_alias_without_brand() {
        // A self-referential alias chain must terminate (depth-bounded) without
        // exempting: the brand never appears in the cycle, so the cast still fires.
        let src = r#"type UserId = string & { readonly __brand: 'UserId' };
type Loop = [Loop];
function makeLoop(raw: string): Loop {
  const id = raw as UserId;
  throw new Error(id);
}"#;
        let diags = run_rule_gated(&Check, src, "src/user/loop.ts");
        assert_eq!(diags.len(), 1, "cast to a brand absent from a cyclic return-type alias must still fire (and terminate), got: {diags:?}");
    }

    #[test]
    fn allows_cast_to_namespace_qualified_local_union_alias() {
        // A namespace-qualified target resolves to its bare local alias; a plain
        // union there is still not branded.
        let src = r#"namespace Schemas {
    export type ColorSchemeId = 'nivo' | 'accent';
}
const v = raw as Schemas.ColorSchemeId;"#;
        let diags = run_rule_gated(&Check, src, "src/components/select.tsx");
        assert!(diags.is_empty(), "cast to a qualified local union alias must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_symbol_for_cast_to_unique_symbol_typeid_brand() {
        // Issue #7171 — effect-ts constructs a nominal `unique symbol` TypeId by
        // casting `Symbol.for(key)` to the brand. The brand has exactly one
        // inhabitant (the symbol itself), so the cast IS the construction, not a
        // validation bypass. Both the wrapped multi-line form and the one-line form.
        let src = r#"const FiberSymbolKey = "effect/Fiber";
export const FiberTypeId: Fiber.FiberTypeId = Symbol.for(
  FiberSymbolKey
) as Fiber.FiberTypeId;
const HashMapSymbolKey = "effect/HashMap";
const HashMapTypeId: HM.TypeId = Symbol.for(HashMapSymbolKey) as HM.TypeId;"#;
        let diags = run_rule_gated(&Check, src, "packages/effect/src/internal/fiber.ts");
        assert!(diags.is_empty(), "Symbol.for(key) cast to a unique-symbol TypeId brand must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_global_symbol_cast_to_typeid_brand() {
        // The global `Symbol(...)` constructor is the same unique-symbol
        // construction as `Symbol.for(...)`; casting its result to the brand is exempt.
        let src = r#"const X = Symbol("k") as Y.TypeId;"#;
        let diags = run_rule_gated(&Check, src, "src/internal/x.ts");
        assert!(diags.is_empty(), "global Symbol(...) cast to a TypeId brand must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_data_brand_cast_of_arbitrary_value() {
        // Negative space: the operand exemption is keyed on a `Symbol()`/`Symbol.*()`
        // call, not on the brand name. Casting an arbitrary runtime value — a string
        // literal, or a member/identifier read — to a data brand still bypasses the
        // validator and must fire, including when the target is a `*.TypeId`.
        let src = r#"const u = '' as UserId;
const e = options.executionId as EntityId;
const m = raw as MachineId;
const t = raw as HM.TypeId;"#;
        let diags = run_rule_gated(&Check, src, "src/domain/ids.ts");
        assert_eq!(diags.len(), 4, "arbitrary-value data-brand casts must still fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_to_enclosing_class_type_parameter() {
        // Issue #7336 — `TId` is a generic type parameter of the enclosing class,
        // a per-instantiation placeholder resolved by TypeScript, not a nominal
        // brand. `id as TId` is the idiomatic way a generic class narrows a
        // computed value to its own type parameter; there is no validator to route
        // through. Its name ends in `Id`, but it resolves to a type parameter.
        let src = r#"class BaseRoute<TId extends string> {
  setId(id: string) {
    const x = id as TId;
    return x;
  }
}"#;
        let diags = run_rule_gated(&Check, src, "src/route.ts");
        assert!(diags.is_empty(), "cast to an enclosing class type parameter must not fire, got: {diags:?}");
    }

    #[test]
    fn allows_cast_to_non_brand_generic_instantiation() {
        // Issue #7336 — `RoutesById<TRouteTree>` is a generic mapped/record type,
        // not a nominal brand: a generic instantiation whose base is not a brand
        // helper. Its base ends in `Id`, but only a brand-helper instantiation
        // carries a brand. (The type parameter `TRouteTree`, not `RoutesById`, so
        // this exercises the generic-instantiation guard, not the type-param one.)
        let src = r#"function f<TRouteTree>(routesById: unknown) {
  return routesById as RoutesById<TRouteTree>;
}"#;
        let diags = run_rule_gated(&Check, src, "src/router.ts");
        assert!(diags.is_empty(), "cast to a non-brand generic instantiation must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_to_brand_helper_instantiation() {
        // Negative space: a brand-helper instantiation (`Brand<string, 'user'>`)
        // IS a nominal brand — its base passes `is_brand_helper_name` — so the
        // generic-instantiation guard must not exempt it; the direct cast fires.
        let src = r#"const b = x as Brand<string, 'user'>;"#;
        let diags = run_rule_gated(&Check, src, "src/domain/b.ts");
        assert_eq!(diags.len(), 1, "cast to a brand-helper instantiation must still fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_to_local_generic_branded_alias() {
        // Negative space: the generic-instantiation guard applies only to targets
        // with no local definition. A locally-defined generic alias that IS branded
        // (`type Id<T> = string & { __brand: T }`) stays nominal — the direct cast
        // to `Id<'user'>` bypasses the validator and must still fire.
        let src = r#"type Id<T> = string & { readonly __brand: T };
const x = raw as Id<'user'>;"#;
        let diags = run_rule_gated(&Check, src, "src/domain/id.ts");
        assert_eq!(diags.len(), 1, "cast to a local generic branded alias must still fire, got: {diags:?}");
    }

    #[test]
    fn flags_cast_to_namespace_qualified_brand_helper_instantiation() {
        // Negative space: a namespace-qualified brand helper (`B.Brand<…>`) is a
        // brand too — the base is compared on its trailing segment — so the
        // generic-instantiation guard must not exempt it; the cast fires.
        let src = r#"const b = x as B.Brand<string, 'user'>;"#;
        let diags = run_rule_gated(&Check, src, "src/domain/b.ts");
        assert_eq!(diags.len(), 1, "cast to a qualified brand-helper instantiation must still fire, got: {diags:?}");
    }
}
