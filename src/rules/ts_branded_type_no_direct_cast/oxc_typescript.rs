//! ts-branded-type-no-direct-cast OXC backend — forbid `as BrandedType`
//! outside a function whose declared return type is that brand (its factory).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSSignature, TSType, TSTypeName, TSTypeOperatorOperator};
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

/// True when a declared return-type annotation IS the brand named `brand`, or a
/// union that includes it (`Brand | undefined`, `Brand | null`).
fn return_type_is_brand(annotation: Option<&oxc_ast::ast::TSTypeAnnotation>, brand: &str) -> bool {
    let Some(ann) = annotation else { return false };
    match &ann.type_annotation {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .any(|ty| type_reference_base_name(ty) == Some(brand)),
        ty => type_reference_base_name(ty) == Some(brand),
    }
}

/// True when an enclosing function/arrow declares `brand` as its return type.
/// Such a function IS the brand's factory: it takes ownership of the raw value
/// and stamps it with the brand, so the `as Brand` cast on the return path is
/// the single sanctioned place the type boundary is crossed.
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
        return_type_is_brand(return_type, bare)
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

        // The name heuristic catches `*Id`/`*Token`/… but plain string-union
        // aliases share that naming (`type ColorSchemeId = 'a' | 'b'`) without
        // being nominal. When the target type is defined in this file, trust its
        // structure: only an intersection that adds a brand marker (or a brand
        // helper instantiation) is genuinely branded. Imported/built-in targets
        // have no local definition to inspect, so they keep the name heuristic.
        if local_alias_is_branded(base_name, semantic) == Some(false) {
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
            severity: Severity::Warning,
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
}
