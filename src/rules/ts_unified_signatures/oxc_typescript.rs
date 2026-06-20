//! ts-unified-signatures OXC backend — flag adjacent function overload signatures
//! in interfaces/type literals that share the same name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    FormalParameters, PropertyKey, TSCallSignatureDeclaration, TSLiteral, TSSignature,
    TSType, TSTypeAnnotation,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

/// The parameter list and declared return type of an overload signature, the only
/// two facets that determine whether a group of overloads can be merged. Lets the
/// unifiability heuristics treat call signatures and named method signatures
/// uniformly.
struct SigShape<'a> {
    params: &'a FormalParameters<'a>,
    return_type: Option<&'a TSTypeAnnotation<'a>>,
}

/// The single string-literal value typing a call signature's first parameter,
/// e.g. `"/geocode"` for `(path: "/geocode"): T`. `None` when the signature does
/// not have exactly one parameter, or that parameter is not a string-literal type.
fn first_param_string_literal<'a>(call: &'a TSCallSignatureDeclaration<'a>) -> Option<&'a str> {
    let [param] = call.params.items.as_slice() else {
        return None;
    };
    let TSType::TSLiteralType(lit) = &param.type_annotation.as_ref()?.type_annotation else {
        return None;
    };
    match &lit.literal {
        TSLiteral::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Path-discriminated dispatchers (e.g. the Azure SDK `Routes` interface) map a
/// distinct string-literal path to a distinct return type per overload. Unifying
/// them would erase the per-path return-type inference, so they are not a smell.
/// True when every call signature is typed by a *distinct* string literal.
fn call_signatures_are_path_discriminated<'a>(
    calls: &[&'a TSCallSignatureDeclaration<'a>],
) -> bool {
    let mut literals = FxHashSet::default();
    for call in calls {
        let Some(literal) = first_param_string_literal(call) else {
            return false;
        };
        if !literals.insert(literal) {
            return false;
        }
    }
    true
}

/// The source text of a signature's declared return type, or `None` when it has
/// no annotation (an inferred return is treated as a distinct return type).
fn return_type_text<'a>(shape: &SigShape<'a>, source: &'a str) -> Option<&'a str> {
    let annotation = shape.return_type?;
    Some(&source[annotation.type_annotation.span().start as usize..annotation.type_annotation.span().end as usize])
}

/// The source text of a signature's parameter list (including the parentheses),
/// e.g. `(pinia: Pinia | undefined)`.
fn params_text<'a>(shape: &SigShape<'a>, source: &'a str) -> &'a str {
    &source[shape.params.span.start as usize..shape.params.span.end as usize]
}

/// Whether the signatures form an "overloaded narrowing" group: every signature
/// has a *distinct* parameter list *and* a *distinct* return type, so each
/// parameter shape maps to its own narrowed return (the
/// `(p: Pinia): Pinia` / `(p: undefined): undefined` / `(p: Pinia | undefined):
/// Pinia | undefined` idiom). Unifying them into one union-parameter signature
/// would collapse the per-variant return into the union and erase the
/// narrowing, so such a group is not a smell.
fn signatures_narrow_return_type<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    if shapes.len() < 2 {
        return false;
    }
    let mut params = FxHashSet::default();
    let mut returns = FxHashSet::default();
    for shape in shapes {
        let Some(return_type) = return_type_text(shape, source) else {
            return false;
        };
        if !params.insert(params_text(shape, source)) || !returns.insert(return_type) {
            return false;
        }
    }
    true
}

/// Whether a group of overload signatures could be merged into one with a union
/// or optional trailing parameter.
///
/// * Parameter counts may differ by at most one — a larger gap would need more
///   than one optional trailing parameter, which the overloads do not express.
/// * When the counts *do* differ, the unified form has to add an optional
///   trailing parameter, so the declared return types must be identical;
///   otherwise the merge would erase the per-overload return-type distinction
///   (the curried zero-arg vs one-arg overload idiom, and the D3-style
///   getter/setter idiom where the 0-arg getter returns the value and the 1-arg
///   setter returns `this`).
/// * When the counts are equal the merge unions a single parameter's type, which
///   is safe only when the signatures are not an overloaded-narrowing group
///   (distinct parameter lists each mapping to a distinct, narrower return).
fn signatures_are_unifiable<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let mut counts = shapes.iter().map(|s| s.params.items.len());
    let Some(first) = counts.next() else {
        return true;
    };
    let (mut min, mut max) = (first, first);
    for count in counts {
        min = min.min(count);
        max = max.max(count);
    }
    if max - min > 1 {
        return false;
    }
    if min == max {
        return !signatures_narrow_return_type(shapes, source);
    }

    let first_return = return_type_text(&shapes[0], source);
    shapes[1..].iter().all(|s| return_type_text(s, source) == first_return)
}

/// One overload group sharing a name: the source offsets where each signature
/// starts (for diagnostics) and the parameter/return shapes (for unifiability).
#[derive(Default)]
struct SigGroup<'a> {
    offsets: Vec<u32>,
    shapes: Vec<SigShape<'a>>,
}

fn collect_signatures<'a>(
    members: &'a [TSSignature<'a>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: FxHashMap<String, SigGroup<'a>> = FxHashMap::default();
    let mut call_sigs: Vec<&TSCallSignatureDeclaration<'a>> = Vec::new();

    for sig in members {
        match sig {
            TSSignature::TSCallSignatureDeclaration(call) => {
                let group = seen.entry("[[call]]".to_string()).or_default();
                group.offsets.push(call.span.start);
                group.shapes.push(SigShape {
                    params: &call.params,
                    return_type: call.return_type.as_deref(),
                });
                call_sigs.push(call);
            }
            TSSignature::TSMethodSignature(method) => {
                let name = match &method.key {
                    PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                    PropertyKey::StringLiteral(s) => s.value.to_string(),
                    _ => continue,
                };
                let group = seen.entry(name).or_default();
                group.offsets.push(method.span.start);
                group.shapes.push(SigShape {
                    params: &method.params,
                    return_type: method.return_type.as_deref(),
                });
            }
            _ => {}
        }
    }

    if call_signatures_are_path_discriminated(&call_sigs) {
        seen.remove("[[call]]");
    }

    seen.retain(|_, group| signatures_are_unifiable(&group.shapes, ctx.source));

    for (name, group) in &seen {
        let offsets = &group.offsets;
        if offsets.len() < 2 {
            continue;
        }
        for &offset in &offsets[1..] {
            let display_name = if name == "[[call]]" {
                "Call signatures".to_string()
            } else {
                format!("`{name}` signatures")
            };
            let (line, _column) = byte_offset_to_line_col(ctx.source, offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "{display_name} can be unified into a single signature \
                     with a union or optional parameter."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
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
            AstKind::TSInterfaceDeclaration(decl) => {
                collect_signatures(&decl.body.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                    collect_signatures(&lit.members, ctx, diagnostics);
                }
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
    fn flags_duplicate_call_signatures() {
        let diags = run_on("interface Foo {\n  (x: string): void;\n  (x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }

    // Regression #1088: Azure SDK `path()` routing — each call signature maps a
    // distinct string-literal path to its own return type. Unifying would erase
    // the per-path return-type inference, so these are not unifiable.
    #[test]
    fn allows_path_discriminated_call_signatures() {
        assert!(
            run_on(
                "export interface Routes {\n  \
                 (path: \"/geocode\"): GetGeocoding;\n  \
                 (path: \"/geocode:batch\"): GetGeocodingBatch;\n  \
                 (path: \"/search/polygon\"): GetPolygon;\n  \
                 (path: \"/reverseGeocode\"): GetReverseGeocoding;\n}"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_call_signatures_with_duplicate_string_literal() {
        let diags = run_on(
            "interface Foo {\n  \
             (path: \"/a\"): X;\n  \
             (path: \"/a\"): Y;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #1977: zustand `Create` — a one-argument signature returning the
    // bound store directly, alongside a zero-argument curried overload returning a
    // factory. Different arity *and* different return types: no single signature
    // (union or optional parameter) expresses both without collapsing the returns.
    #[test]
    fn allows_curried_zero_arg_vs_one_arg_overload() {
        assert!(
            run_on(
                "type Create = {\n  \
                 <T, Mos extends [string, unknown][] = []>(initializer: StateCreator<T, [], Mos>): UseBoundStore<Mutate<StoreApi<T>, Mos>>;\n  \
                 <T>(): <Mos extends [string, unknown][] = []>(initializer: StateCreator<T, [], Mos>) => UseBoundStore<Mutate<StoreApi<T>, Mos>>;\n}"
            )
            .is_empty()
        );
    }

    // Guard: call signatures differing by exactly one trailing optional parameter
    // and sharing the same return type are genuinely unifiable, so still fire.
    #[test]
    fn flags_trailing_optional_parameter_overload() {
        let diags = run_on(
            "interface Foo {\n  \
             (a: number): void;\n  \
             (a: number, b?: string): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #1721: pinia `_SetActivePinia` — each call signature maps a
    // distinct parameter type to its own narrowed return type. Unifying them into
    // `(pinia: Pinia | undefined): Pinia | undefined` would erase the per-variant
    // return-type narrowing, so these are not unifiable.
    #[test]
    fn allows_overloaded_narrowing_return_types() {
        assert!(
            run_on(
                "interface _SetActivePinia {\n  \
                 (pinia: Pinia): Pinia\n  \
                 (pinia: undefined): undefined\n  \
                 (pinia: Pinia | undefined): Pinia | undefined\n}"
            )
            .is_empty()
        );
    }

    // Guard: equal param counts with distinct parameter types but the *same*
    // return type are genuinely unifiable into one union-parameter signature, so
    // still fire — the narrowing exemption must not swallow this real smell.
    #[test]
    fn flags_distinct_params_with_shared_return_type() {
        let diags = run_on(
            "interface Foo {\n  \
             (x: string): void;\n  \
             (x: number): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: param counts differing by more than one cannot be merged with a
    // single optional trailing parameter, so they are not a smell.
    #[test]
    fn allows_call_signatures_differing_by_more_than_one_param() {
        assert!(
            run_on(
                "interface Foo {\n  \
                 (a: number): void;\n  \
                 (a: number, b: string, c: string): void;\n}"
            )
            .is_empty()
        );
    }

    // Regression #4751: D3-style getter/setter method overloads — a 0-arg getter
    // returning the current value and a 1-arg setter returning `this` for
    // chaining. Different arity *and* different return types: no single signature
    // expresses both without collapsing the per-arity returns into a union.
    #[test]
    fn allows_d3_getter_setter_method_overloads() {
        assert!(
            run_on(
                "export interface Cloud<T extends CloudWord> {\n  \
                 timeInterval(): number;\n  \
                 timeInterval(interval: number): Cloud<T>;\n  \
                 size(): [number, number];\n  \
                 size(size: [number, number]): Cloud<T>;\n  \
                 rotate(): (datum: T, index: number) => number;\n  \
                 rotate(rotate: number | ((datum: T, index: number) => number)): Cloud<T>;\n}"
            )
            .is_empty()
        );
    }

    // Guard: a method overload pair differing only by a single parameter and
    // sharing the same return type is genuinely unifiable, so still fires.
    #[test]
    fn flags_unifiable_method_overloads() {
        let diags = run_on(
            "interface Foo {\n  \
             bar(a: string): void;\n  \
             bar(a: number): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }
}
