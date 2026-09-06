//! ts-unified-signatures OXC backend — flag overload signature groups that one
//! signature with a union or optional parameter would express, in every carrier
//! TypeScript spells overloads with: interfaces, type literals, class bodies,
//! and function declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ClassBody, ClassElement, Declaration, FormalParameters, Function, MethodDefinition,
    MethodDefinitionKind, PropertyKey, Statement, TSCallSignatureDeclaration, TSLiteral,
    TSSignature, TSType, TSTypeAnnotation, TSTypeParameterDeclaration,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

/// The group key of a type's call signatures, which have no name of their own.
const CALL_SIGNATURE_KEY: &str = "[[call]]";

/// The type parameters, parameter list, and declared return type of an overload
/// signature — the facets that determine whether a group of overloads can be
/// merged. Lets the unifiability heuristics treat call signatures, method
/// signatures, class methods and function declarations uniformly.
struct SigShape<'a> {
    type_params: Option<&'a TSTypeParameterDeclaration<'a>>,
    params: &'a FormalParameters<'a>,
    return_type: Option<&'a TSTypeAnnotation<'a>>,
}

impl<'a> SigShape<'a> {
    /// The shape of a body-less `Function` — the node behind both a function
    /// overload declaration and a class-method overload.
    fn from_function(func: &'a Function<'a>) -> Self {
        Self {
            type_params: func.type_parameters.as_deref(),
            params: &func.params,
            return_type: func.return_type.as_deref(),
        }
    }
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

/// The source text of a signature's type-parameter list (including the angle
/// brackets), e.g. `<$Output extends AsyncIterable<any, void, any>>`, or `None`
/// when the signature declares none. Two overloads whose parameter lists are
/// textually identical can still be distinguished by their type-parameter
/// constraints, so this is part of a signature's input identity.
fn type_params_text<'a>(shape: &SigShape<'a>, source: &'a str) -> Option<&'a str> {
    let decl = shape.type_params?;
    Some(&source[decl.span.start as usize..decl.span.end as usize])
}

/// The source text of the type of a signature's rest parameter, e.g. `number[]`
/// for `(...args: number[])`. `None` when the signature declares no rest
/// parameter, or declares an untyped one.
fn rest_param_type_text<'a>(shape: &SigShape<'a>, source: &'a str) -> Option<&'a str> {
    let span = shape
        .params
        .rest
        .as_ref()?
        .type_annotation
        .as_ref()?
        .type_annotation
        .span();
    Some(&source[span.start as usize..span.end as usize])
}

/// The source text of the type annotation of the parameter at `index`, or `None`
/// when the signature has no parameter there or that parameter is untyped. Names
/// are excluded so positions compare by type alone.
fn param_type_text<'a>(shape: &SigShape<'a>, index: usize, source: &'a str) -> Option<&'a str> {
    let span = shape
        .params
        .items
        .get(index)?
        .type_annotation
        .as_ref()?
        .type_annotation
        .span();
    Some(&source[span.start as usize..span.end as usize])
}

/// Which of the first `count` parameter positions have a parameter type text not
/// shared by every signature in the group. A unified signature can union at most one
/// position, so two or more differing positions (the equal-arity case) block the
/// merge; for differing-arity overloads any differing *shared* position blocks it,
/// because adding only a trailing parameter requires the shorter signature to be a
/// type-prefix of the longer.
fn differing_param_positions<'a>(
    shapes: &[SigShape<'a>],
    count: usize,
    source: &'a str,
) -> Vec<usize> {
    (0..count)
        .filter(|&pos| {
            let baseline = param_type_text(&shapes[0], pos, source);
            shapes[1..]
                .iter()
                .any(|s| param_type_text(s, pos, source) != baseline)
        })
        .collect()
}

/// The union type a merged signature would declare at the group's single
/// differing parameter position — every signature's type text there, in source
/// order, without repeats. `None` when the signatures do not share an arity,
/// when no position or more than one differs, or when that position is untyped
/// in some signature: the merge is then not a single union parameter, and the
/// diagnostic has no union to name.
fn union_parameter_text<'a>(shapes: &[SigShape<'a>], source: &'a str) -> Option<String> {
    let arity = shapes.first()?.params.items.len();
    if shapes.iter().any(|s| s.params.items.len() != arity) {
        return None;
    }
    let [position] = differing_param_positions(shapes, arity, source)[..] else {
        return None;
    };
    let mut parts: Vec<&str> = Vec::new();
    for shape in shapes {
        let text = param_type_text(shape, position, source)?;
        if !parts.contains(&text) {
            parts.push(text);
        }
    }
    Some(parts.join(" | "))
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

/// Whether every signature in the group declares the same return type text (all
/// annotated with equal text, or all unannotated).
fn all_returns_identical<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let first = return_type_text(&shapes[0], source);
    shapes[1..]
        .iter()
        .all(|s| return_type_text(s, source) == first)
}

/// Whether the group binds its generics identically: one type-parameter list text
/// across all of it (or none anywhere), so same count, names, constraints and
/// defaults. Overloads that bind them differently are not interchangeable —
/// unioning one value parameter leaves a single type-parameter list, which cannot
/// reproduce per-overload inference.
fn all_type_params_identical<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let first = type_params_text(&shapes[0], source);
    shapes[1..]
        .iter()
        .all(|s| type_params_text(s, source) == first)
}

/// Whether one and the same variadic tail closes each of the group's parameter
/// lists — a rest parameter of equal type everywhere, or none anywhere. A rest
/// parameter accepts an unbounded number of arguments, so it is neither the one
/// optional parameter a merged signature may add nor a position a union can
/// express: the vueuse `useAverage(array: T[])` / `useAverage(...args: T[])` pair
/// states two call conventions, not two types at one position.
fn all_rest_params_identical<'a>(shapes: &[SigShape<'a>], source: &'a str) -> bool {
    let declares_rest = shapes[0].params.rest.is_some();
    let first = rest_param_type_text(&shapes[0], source);
    shapes[1..].iter().all(|s| {
        s.params.rest.is_some() == declares_rest && rest_param_type_text(s, source) == first
    })
}

/// Whether a group of overload signatures could be merged into one with a union
/// or optional trailing parameter.
///
/// * The type-parameter lists must be identical. Overloads that bind their generics
///   differently — a differing count, name, constraint or default — each declare
///   their own inference rule, and a merged signature has only one type-parameter
///   list to state. The vueuse `useStorage<T>(key, defaults: T)` /
///   `useStorage<T = unknown>(key, defaults: null)` pair is the canonical case: the
///   first infers `T` *from* the differing parameter, the second has no inference
///   site and falls back to its default, so the merged `defaults: T | null` infers
///   `T = null` for the `null` call the second overload types as `unknown`. The DOM
///   `addEventListener` idiom, where each overload correlates a target constraint
///   with its event map, is the same disqualifier.
/// * The variadic tails must match. An overload that ends in a rest parameter takes
///   an unbounded argument list, which is neither the single optional parameter a
///   merged signature may add nor a position a union can widen.
/// * Parameter counts may differ by at most one — a larger gap would need more
///   than one optional trailing parameter, which the overloads do not express.
/// * When the counts *do* differ, the unified form has to add an optional
///   trailing parameter, so the declared return types must be identical (otherwise
///   the merge would erase the per-overload return-type distinction — the curried
///   zero-arg vs one-arg overload idiom, and the D3-style getter/setter idiom where
///   the 0-arg getter returns the value and the 1-arg setter returns `this`) *and*
///   every shared parameter position must have identical type text, since adding
///   only a trailing parameter requires the shorter signature to be a type-prefix of
///   the longer; a differing shared position (the kysely `innerJoin(table, k1, k2)`
///   vs `innerJoin(table, callback)` idiom) would force a union at an internal
///   position, over-permitting combinations the overloads reject.
/// * When the counts are equal the merge unions a single parameter's type. This is
///   unsafe — so the group is not unifiable — when the signatures form an
///   overloaded-narrowing group (distinct parameter lists each mapping to a
///   distinct, narrower return), or their return types are not all identical while a
///   parameter position differs, because then each overload narrows the return
///   conditionally on its input — the trpc `useTRPCInfiniteQuery` idiom, where the
///   `opts` shape selects between `DefinedUseInfiniteQueryResult` and
///   `UseInfiniteQueryResult` — which a single union return cannot express, or they
///   differ at two or more parameter positions (the EventEmitter idiom, where a
///   string-literal event and its narrowed listener both differ from the catch-all overload; a
///   per-position union would admit listener/event combinations the overloads
///   reject). Overloads with identical inputs but differing returns are a redundant
///   duplicate whose only merge — a union return — is itself the smell, so they
///   still fire.
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
    if !all_type_params_identical(shapes, source) || !all_rest_params_identical(shapes, source) {
        return false;
    }
    if min == max {
        if signatures_narrow_return_type(shapes, source) {
            return false;
        }
        let differing = differing_param_positions(shapes, min, source).len();
        if !all_returns_identical(shapes, source) && differing > 0 {
            return false;
        }
        return differing < 2;
    }

    if !all_returns_identical(shapes, source) {
        return false;
    }
    differing_param_positions(shapes, min, source).is_empty()
}

/// One overload group sharing a name: the source offsets where each signature
/// starts (for diagnostics) and the parameter/return shapes (for unifiability).
#[derive(Default)]
struct SigGroup<'a> {
    offsets: Vec<u32>,
    shapes: Vec<SigShape<'a>>,
}

impl<'a> SigGroup<'a> {
    fn push(&mut self, offset: u32, shape: SigShape<'a>) {
        self.offsets.push(offset);
        self.shapes.push(shape);
    }
}

/// The overload groups of one carrier, keyed by the name the overloads share
/// ([`CALL_SIGNATURE_KEY`] for call signatures, `static m` for a static method).
type SigGroups<'a> = FxHashMap<String, SigGroup<'a>>;

/// The name a member groups its overloads under. A computed key has no
/// statically-known name, so it cannot be matched against a sibling.
fn member_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

/// Emit one diagnostic per unifiable group, anchored on the group's first
/// signature and naming the union the merge proposes. An overload group is one
/// finding: the same advice reported per mergeable pair repeats positions once
/// the group has three members, and each pair names only part of the union the
/// reader has to write.
fn report_unifiable_groups(
    groups: SigGroups<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut findings: Vec<(u32, String)> = groups
        .into_iter()
        .filter(|(_, group)| {
            group.offsets.len() >= 2 && signatures_are_unifiable(&group.shapes, ctx.source)
        })
        .map(|(name, group)| {
            let subject = if name == CALL_SIGNATURE_KEY {
                "Call signatures".to_owned()
            } else {
                format!("`{name}` signatures")
            };
            let merge = match union_parameter_text(&group.shapes, ctx.source) {
                Some(union) => format!("taking `{union}`"),
                None => "with a union or optional parameter".to_owned(),
            };
            (
                group.offsets[0],
                format!("{subject} can be unified into a single signature {merge}."),
            )
        })
        .collect();
    findings.sort_unstable_by_key(|(offset, _)| *offset);

    for (offset, message) in findings {
        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Error,
            span: None,
        });
    }
}

/// The overload groups of an interface body or type literal: its call
/// signatures under one key, its method signatures under their names.
/// Path-discriminated call signatures are dropped from the result.
fn collect_signatures<'a>(members: &'a [TSSignature<'a>]) -> SigGroups<'a> {
    let mut groups = SigGroups::default();
    let mut call_sigs: Vec<&TSCallSignatureDeclaration<'a>> = Vec::new();

    for sig in members {
        match sig {
            TSSignature::TSCallSignatureDeclaration(call) => {
                groups.entry(CALL_SIGNATURE_KEY.to_owned()).or_default().push(
                    call.span.start,
                    SigShape {
                        type_params: call.type_parameters.as_deref(),
                        params: &call.params,
                        return_type: call.return_type.as_deref(),
                    },
                );
                call_sigs.push(call);
            }
            TSSignature::TSMethodSignature(method) => {
                let Some(name) = member_name(&method.key) else {
                    continue;
                };
                groups.entry(name).or_default().push(
                    method.span.start,
                    SigShape {
                        type_params: method.type_parameters.as_deref(),
                        params: &method.params,
                        return_type: method.return_type.as_deref(),
                    },
                );
            }
            _ => {}
        }
    }

    if call_signatures_are_path_discriminated(&call_sigs) {
        groups.remove(CALL_SIGNATURE_KEY);
    }
    groups
}

/// The group key and node of a class element that is an overload signature: a
/// body-less method or constructor. Accessors cannot be overloaded, and a static
/// and an instance method of the same name are two members rather than two
/// overloads, so the key carries `static`.
fn class_method_overload<'a>(
    element: &'a ClassElement<'a>,
) -> Option<(String, &'a MethodDefinition<'a>)> {
    let ClassElement::MethodDefinition(method) = element else {
        return None;
    };
    if method.value.body.is_some()
        || !matches!(
            method.kind,
            MethodDefinitionKind::Method | MethodDefinitionKind::Constructor
        )
    {
        return None;
    }
    let name = member_name(&method.key)?;
    let key = if method.r#static {
        format!("static {name}")
    } else {
        name
    };
    Some((key, method))
}

/// The overload groups of a class body, keyed by [`class_method_overload`].
fn collect_class_methods<'a>(body: &'a ClassBody<'a>) -> SigGroups<'a> {
    let mut groups = SigGroups::default();
    for (key, method) in body.body.iter().filter_map(class_method_overload) {
        groups
            .entry(key)
            .or_default()
            .push(method.span.start, SigShape::from_function(&method.value));
    }
    groups
}

/// The name and node of a statement that declares an overload signature: a
/// function declaration with no body — an implementation signature is not one of
/// the overloads it implements. Both `function f(…): T` and
/// `export function f(…): T` carry the declaration, so both spellings are read.
fn function_overload<'a>(statement: &'a Statement<'a>) -> Option<(&'a str, &'a Function<'a>)> {
    let func = match statement {
        Statement::FunctionDeclaration(func) => func,
        Statement::ExportNamedDeclaration(export) => match export.declaration.as_ref()? {
            Declaration::FunctionDeclaration(func) => func,
            _ => return None,
        },
        _ => return None,
    };
    if func.body.is_some() {
        return None;
    }
    Some((func.id.as_ref()?.name.as_str(), func))
}

/// The overload groups of a statement list, keyed by function name.
fn collect_function_declarations<'a>(statements: &'a [Statement<'a>]) -> SigGroups<'a> {
    let mut groups = SigGroups::default();
    for (name, func) in statements.iter().filter_map(function_overload) {
        groups
            .entry(name.to_owned())
            .or_default()
            .push(func.span.start, SigShape::from_function(func));
    }
    groups
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TSInterfaceDeclaration,
            AstType::TSTypeAliasDeclaration,
            AstType::ClassBody,
            AstType::Program,
            AstType::TSModuleBlock,
        ]
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
                report_unifiable_groups(collect_signatures(&decl.body.body), ctx, diagnostics);
            }
            AstKind::ClassBody(body) => {
                report_unifiable_groups(collect_class_methods(body), ctx, diagnostics);
            }
            AstKind::Program(program) => {
                report_unifiable_groups(
                    collect_function_declarations(&program.body),
                    ctx,
                    diagnostics,
                );
            }
            AstKind::TSModuleBlock(block) => {
                report_unifiable_groups(
                    collect_function_declarations(&block.body),
                    ctx,
                    diagnostics,
                );
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                    report_unifiable_groups(collect_signatures(&lit.members), ctx, diagnostics);
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

    // Regression #6254: Node.js EventEmitter-style overloads — a string-literal
    // event paired with a narrowed listener, then a catch-all `string` event with a
    // generic listener, both returning `this`. Both parameters differ across the
    // pair, so a per-position union would let `(...args) => void` pair with the
    // `'error'` event, which the overloads reject. Two differing positions => not
    // unifiable.
    #[test]
    fn allows_event_emitter_overloads() {
        assert!(
            run_on(
                "export interface TediousConnection {\n  \
                 off(event: 'error', listener: (error: unknown) => void): this\n  \
                 off(event: string, listener: (...args: any[]) => void): this\n  \
                 on(event: 'error', listener: (error: unknown) => void): this\n  \
                 on(event: string, listener: (...args: any[]) => void): this\n  \
                 once(event: 'end', listener: () => void): this\n  \
                 once(event: string, listener: (...args: any[]) => void): this\n}"
            )
            .is_empty()
        );
    }

    // Guard: an equal-arity pair differing at two parameter positions cannot be
    // merged without a per-position union that admits `f(number, number)` and
    // `f(string, string)`, which neither overload declares. Not unifiable.
    #[test]
    fn allows_two_position_differing_overloads() {
        assert!(
            run_on(
                "interface Foo {\n  \
                 f(a: number, b: string): void;\n  \
                 f(a: string, b: number): void;\n}"
            )
            .is_empty()
        );
    }

    // Guard: a single differing parameter position with a shared return type is
    // genuinely unifiable into `x: number | string`, so still fires.
    #[test]
    fn flags_single_position_differing_overloads() {
        let diags = run_on(
            "interface Foo {\n  \
             f(x: number): void;\n  \
             f(x: string): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: a string-literal vs `string` first parameter where the listener type is
    // *identical* differs at exactly one position, so the literal overload is
    // subsumed and the pair is unifiable — must still fire (distinguishes the fix
    // from a naive "literal-vs-string first param => exempt" carve-out).
    #[test]
    fn flags_literal_vs_string_with_identical_remaining_param() {
        let diags = run_on(
            "interface Foo {\n  \
             off(event: 'error', listener: (e: unknown) => void): this\n  \
             off(event: string, listener: (e: unknown) => void): this\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Regression #6255: kysely `innerJoin` — a 3-param `(table, k1, k2)` overload and
    // a 2-param `(table, callback)` overload sharing the same return type. The counts
    // differ by one, but shared position 1 differs (`K1` vs `FN`), so the shorter
    // signature is not a type-prefix of the longer: adding only a trailing optional
    // parameter cannot express both without a union at an internal position that
    // over-permits. Not unifiable.
    #[test]
    fn allows_diff_one_overloads_with_differing_shared_position() {
        assert!(
            run_on(
                "interface SelectQueryBuilder<DB, TB, O> {\n  \
                 innerJoin<TE extends TableExpression<DB, TB>, K1 extends JoinReferenceExpression<DB, TB, TE>, K2 extends JoinReferenceExpression<DB, TB, TE>>(table: TE, k1: K1, k2: K2): SelectQueryBuilderWithInnerJoin<DB, TB, O, TE>\n  \
                 innerJoin<TE extends TableExpression<DB, TB>, const FN extends JoinCallbackExpression<DB, TB, TE>>(table: TE, callback: FN): SelectQueryBuilderWithInnerJoin<DB, TB, O, TE>\n}"
            )
            .is_empty()
        );
    }

    // Regression #7117 (Case 1): trpc `subscription` — two equal-arity method
    // overloads with textually-identical parameters but different method-level
    // generic constraints and different return types. The params compare as 0
    // differing positions, but the returns differ, so each overload narrows its
    // return conditionally on `$Output`'s constraint — a single union-parameter
    // signature cannot express that. Not unifiable.
    #[test]
    fn allows_equal_arity_overloads_with_differing_return_types() {
        assert!(
            run_on(
                "interface ProcedureBuilder<TContext> {\n  \
                 subscription<$Output extends AsyncIterable<any, void, any>>(resolver: ProcedureResolver<TContext, $Output>): SubscriptionProcedure<$Output>;\n  \
                 subscription<$Output extends Observable<any, any>>(resolver: ProcedureResolver<TContext, $Output>): LegacyObservableSubscriptionProcedure<$Output>;\n}"
            )
            .is_empty()
        );
    }

    // Regression #7117 (Case 2): trpc `useTRPCInfiniteQuery` — three equal-arity
    // call signatures whose returns are mixed (overload 1 returns
    // `DefinedUseInfiniteQueryResult`, overloads 2–3 share `UseInfiniteQueryResult`).
    // The returns are neither all-distinct (so the narrowing exemption does not
    // apply) nor all-identical, so the group conditionally narrows the return on the
    // `opts` shape, which a union `opts` parameter cannot replicate. Not unifiable.
    #[test]
    fn allows_equal_arity_call_signatures_with_mixed_return_types() {
        assert!(
            run_on(
                "interface useTRPCInfiniteQuery<TDef> {\n  \
                 <TData>(input: TInput, opts: DefinedInitialDataInfiniteOptions<TData>): DefinedUseInfiniteQueryResult<TData>;\n  \
                 <TData>(input: TInput, opts?: UndefinedInitialDataInfiniteOptions<TData>): UseInfiniteQueryResult<TData>;\n  \
                 <TData>(input: TInput, opts?: UseInfiniteQueryOptions<TData>): UseInfiniteQueryResult<TData>;\n}"
            )
            .is_empty()
        );
    }

    // ── overload carriers other than interfaces and type literals (#8181) ────

    /// The jsdiff `Diff.diff` overload group, whose five signatures map each
    /// option shape to its own narrowed return.
    const NARROWED_RETURN_OVERLOADS: &str = "\
diff(a: string, b: string, o: Cb): undefined;
diff(a: string, b: string, o: OptsAbortable & { callback: Cb }): undefined;
diff(a: string, b: string, o: OptsSync & { callback: Cb }): undefined;
diff(a: string, b: string, o: OptsAbortable): string[] | undefined;
diff(a: string, b: string, o?: OptsSync): string[];";

    // Regression #8181: kpdecker/jsdiff `Diff.diff` — the same overload group the
    // interface spelling already exempts, spelled as class methods. Each option
    // shape maps to its own return, so a union parameter would erase the
    // correlation callers rely on.
    #[test]
    fn allows_class_method_overloads_with_narrowed_return_types() {
        assert!(
            run_on(&format!(
                "export class Diff {{\n  {NARROWED_RETURN_OVERLOADS}\n  \
                 diff(a: string, b: string, o?: any): string[] | undefined {{ return []; }}\n}}"
            ))
            .is_empty()
        );
    }

    // Regression #8181: the same group spelled as function declarations.
    #[test]
    fn allows_function_overloads_with_narrowed_return_types() {
        let source = NARROWED_RETURN_OVERLOADS.replace("diff(", "export function diff(");
        assert!(run_on(&source).is_empty());
    }

    // Guard: a genuinely mergeable class-method overload pair fires — once, from
    // the class body, with the implementation signature left out of the group.
    #[test]
    fn flags_unifiable_class_method_overloads() {
        let diags = run_on(
            "export class C {\n  \
             m(x: string): void;\n  \
             m(x: number): void;\n  \
             m(x: string | number): void {}\n}",
        );
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("`string | number`"),
            "{}",
            diags[0].message
        );
    }

    // Guard: a genuinely mergeable function-declaration overload pair fires once.
    #[test]
    fn flags_unifiable_function_overloads() {
        let diags = run_on(
            "export function foo(x: string): void;\n\
             export function foo(x: number): void;\n\
             export function foo(x: string | number): void {}\n",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: overloads declared inside an ambient module block are read too.
    #[test]
    fn flags_unifiable_overloads_in_a_module_block() {
        let diags = run_on(
            "declare module 'pkg' {\n  \
             export function f(x: string): void;\n  \
             export function f(x: number): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // A static and an instance method of the same name are two members, not two
    // overloads: they cannot be merged into one signature.
    #[test]
    fn allows_same_name_static_and_instance_methods() {
        assert!(
            run_on(
                "declare class C {\n  \
                 static m(x: string): void;\n  \
                 m(x: number): void;\n}"
            )
            .is_empty()
        );
    }

    // Regression #5506: vobyjs/voby `useEventListener` — each overload pairs a
    // target constraint (`T extends Window`) with the matching event map
    // (`U extends keyof WindowEventMap`). The correlated constraints differ per
    // overload, so no single union-parameter signature expresses them.
    #[test]
    fn allows_target_type_discriminated_dom_overloads() {
        assert!(
            run_on(
                "function useEventListener<T extends Window, U extends keyof WindowEventMap>(target: T, event: U): Disposer;\n\
                 function useEventListener<T extends Document, U extends keyof DocumentEventMap>(target: T, event: U): Disposer;\n\
                 function useEventListener<T extends HTMLElement, U extends keyof HTMLElementEventMap>(target: T, event: U): Disposer;\n\
                 function useEventListener(target: unknown, event: string): Disposer {\n  return () => {};\n}\n"
            )
            .is_empty()
        );
    }

    // Guard: overloads whose generic constraints are identical differ only in a
    // unionizable value parameter, so they remain genuinely mergeable.
    #[test]
    fn flags_function_overloads_with_identical_generic_constraints() {
        let diags = run_on(
            "function wrap<T extends object>(x: T, k: string): T;\n\
             function wrap<T extends object>(x: T, k: number): T;\n\
             function wrap<T extends object>(x: T, k: string | number): T {\n  return x;\n}\n",
        );
        assert_eq!(diags.len(), 1);
    }

    // A group mixing a constrained-generic member with a plain one binds its
    // type parameters differently per overload: a meaningful generic constraint
    // and its absence are not unifiable into one signature.
    #[test]
    fn allows_mixed_generic_and_plain_function_overloads() {
        assert!(
            run_on(
                "function pick<T extends object>(x: T): T;\n\
                 function pick(x: string): string;\n\
                 function pick(x: unknown): unknown {\n  return x;\n}\n"
            )
            .is_empty()
        );
    }

    // Regression #8181: an overload group is one finding. Reporting per mergeable
    // pair emits `n choose 2` diagnostics for at most `n - 1` available merges,
    // and anchors the later ones on a position it already used.
    #[test]
    fn reports_a_five_member_group_once() {
        let diags = run_on(
            "export function e(x: 'a'): void\n\
             export function e(x: 'b'): void\n\
             export function e(x: 'c'): void\n\
             export function e(x: 'd'): void\n\
             export function e(x: 'e'): void\n\
             export function e(x: string): void { void x }\n",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 1);
        assert!(
            diags[0].message.contains("`'a' | 'b' | 'c' | 'd' | 'e'`"),
            "{}",
            diags[0].message
        );
    }

    // ── differing type-parameter lists, and the union the merge proposes (#8282) ─

    // Regression #8282: vueuse `useStorage` — `<T>` infers the stored type from
    // `defaults`, while `<T = unknown>` has no inference site there and falls back
    // to its default. The merged `defaults: T | null` infers `T = null` for the
    // call the second overload types as `unknown`, which tsgo rejects at the call
    // site: the merge is a breaking change to the public type, not a refactor.
    #[test]
    fn allows_overloads_with_differing_type_parameter_defaults() {
        assert!(
            run_on(
                "export function c<T>(key: string, defaults: T): Ref<T>\n\
                 export function c<T = unknown>(key: string, defaults: null): Ref<T>\n\
                 export function c<T>(key: string, defaults: T | null): Ref<T> {\n  \
                 return { value: defaults as T }\n}\n"
            )
            .is_empty()
        );
    }

    // Guard: the same pair with *identical* type-parameter lists states one
    // inference rule for both overloads, so the union merge is sound and fires —
    // the discriminator is the type-parameter list, not the `null` parameter.
    #[test]
    fn flags_overloads_with_identical_type_parameters_and_null_param() {
        let diags = run_on(
            "export function c<T>(key: string, defaults: T): Ref<T>\n\
             export function c<T>(key: string, defaults: null): Ref<T>\n\
             export function c<T>(key: string, defaults: T | null): Ref<T> {\n  \
             return { value: defaults as T }\n}\n",
        );
        assert_eq!(diags.len(), 1);
    }

    // Overloads that name their type parameter differently declare two distinct
    // bindings; a merged signature has one list to state.
    #[test]
    fn allows_overloads_with_differing_type_parameter_names() {
        assert!(
            run_on(
                "export function c<T>(x: T): R<T>\n\
                 export function c<U>(x: null): R<U>\n\
                 export function c(x: unknown): unknown { return x }\n"
            )
            .is_empty()
        );
    }

    // Regression #8282: three mergeable overloads are one finding that names the
    // whole union, not one per pair proposing a partial merge.
    #[test]
    fn reports_a_three_member_group_once_with_the_full_union() {
        let diags = run_on(
            "export function e(x: string): void\n\
             export function e(x: number): void\n\
             export function e(x: boolean): void\n\
             export function e(x: any): void { void x }\n",
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 1);
        assert!(
            diags[0].message.contains("`string | number | boolean`"),
            "{}",
            diags[0].message
        );
    }

    // Overloads of differing arity merge with an optional trailing parameter, so
    // there is no single position to union and no union to name.
    #[test]
    fn allows_function_overloads_of_differing_arity() {
        assert!(
            run_on(
                "export function b(url: string, opts: FetchOpts): void\n\
                 export function b(url: string, init: RequestInit, opts?: FetchOpts): void\n\
                 export function b(url: string, ...args: any[]): void { void url; void args }\n"
            )
            .is_empty()
        );
    }

    // vueuse `useAverage` — an array overload beside a variadic one. The two state
    // two call conventions, `useAverage([a, b])` and `useAverage(a, b)`; neither a
    // union at one position nor an optional trailing parameter expresses both.
    #[test]
    fn allows_overloads_differing_by_a_rest_parameter() {
        assert!(
            run_on(
                "export function useAverage(array: MaybeRefOrGetter<number>[]): ComputedRef<number>\n\
                 export function useAverage(...args: MaybeRefOrGetter<number>[]): ComputedRef<number>\n\
                 export function useAverage(...args: any[]): ComputedRef<number> {\n  \
                 return { value: args.length }\n}\n"
            )
            .is_empty()
        );
    }

    // Guard: a zero-argument overload beside a one-argument one is genuinely
    // `f(a?: string)` — the rest-parameter guard must not swallow it.
    #[test]
    fn flags_zero_arg_and_one_arg_function_overloads() {
        let diags = run_on(
            "export function opt(): number\n\
             export function opt(a: string): number\n\
             export function opt(a?: string): number { return a ? a.length : 0 }\n",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: overloads that share the same variadic tail still differ at one
    // fixed position, so they remain mergeable.
    #[test]
    fn flags_overloads_sharing_a_rest_parameter() {
        let diags = run_on(
            "export function tag(first: string, ...rest: number[]): void\n\
             export function tag(first: number, ...rest: number[]): void\n\
             export function tag(first: any, ...rest: number[]): void { void first; void rest }\n",
        );
        assert_eq!(diags.len(), 1);
    }

    // Overloads whose returns are narrowed per parameter type stay exempt when
    // spelled as function declarations.
    #[test]
    fn allows_function_overloads_with_distinct_return_types() {
        assert!(
            run_on(
                "export function d(x: string): string\n\
                 export function d(x: number): number\n\
                 export function d(x: any): any { return x }\n"
            )
            .is_empty()
        );
    }

    // No two diagnostics ever share a position: each is a whole overload group,
    // so a reader fixing one is not told the same thing again, and downstream
    // deduplication by `(path, line, column)` drops nothing.
    #[test]
    fn never_reports_two_diagnostics_at_one_position() {
        let diags = run_on(
            "interface I { m(x: string): void; m(x: number): void; n(x: string): void; n(x: number): void }\n\
             export class C {\n  \
             p(x: string): void;\n  \
             p(x: number): void;\n  \
             p(x: boolean): void;\n  \
             p(x: any): void {}\n}\n\
             export function q(x: string): void\n\
             export function q(x: number): void\n\
             export function q(x: boolean): void\n\
             export function q(x: any): void { void x }\n",
        );
        assert_eq!(diags.len(), 4, "{diags:?}");
        let mut positions: Vec<(usize, usize)> = diags.iter().map(|d| (d.line, d.column)).collect();
        positions.sort_unstable();
        let mut deduped = positions.clone();
        deduped.dedup();
        assert_eq!(positions, deduped, "duplicate positions: {positions:?}");
    }
}
