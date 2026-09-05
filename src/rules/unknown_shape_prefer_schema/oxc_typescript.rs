//! unknown-shape-prefer-schema oxc backend — flag functions that validate the
//! shape of a value whose binding is explicitly annotated `unknown` (parameter
//! or variable) with hand-written checks: `typeof v.prop === '<primitive>'`,
//! `v.prop === <literal>`, `'key' in v`, or (twice or more) `v === <literal>`.
//! Such a binding is declared boundary data; its shape belongs in a schema.
//! One diagnostic per enclosing function, at the first check. An object-ness
//! gate alone (`typeof v === 'object' && v !== null`) and a lone
//! `typeof v === '<primitive>'` narrowing stay silent — narrowing utilities are
//! fine, their shape-checking callers are not. Values without the annotation
//! (untyped or typed parameters, catch bindings) are out of scope: only an
//! explicit `unknown` proves the author knew the value was unvalidated.
//!
//! Reads inside a branch guarded by a concrete-type narrowing of the same
//! binding — `v instanceof C`, `Array.isArray(v)` — are typed reads of a
//! declared member, not probes into an unvalidated bag, so they do not count.
//! An object-ness gate narrows to no named type and keeps its branch in scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, identifier_is_unshadowed_global, span_contains};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BinaryExpression, BinaryOperator, Expression, IdentifierReference, LogicalOperator,
    TSType, UnaryOperator,
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// `typeof` results that validate a property's shape. `function` is feature
/// detection and `undefined` is presence checking; neither belongs to a schema.
const SHAPE_PRIMITIVES: &[&str] = &["string", "number", "boolean", "object", "symbol", "bigint"];

/// One validation read of an `unknown`-annotated binding.
enum ReadKind {
    /// A property-level check: `typeof v.prop === '...'`, `v.prop === <lit>`,
    /// or `'key' in v`. One is enough to prove shape validation.
    Member,
    /// The binding itself compared to a literal (`v === 'image'`). One is a
    /// sentinel check; two or more form a hand-rolled literal union.
    BaseLiteral,
}

struct Read {
    kind: ReadKind,
    span: Span,
}

/// Where a binding carries a concrete type: the branch spans guarded by a
/// narrowing that names one, per narrowed binding.
type NarrowedBranches = FxHashMap<SymbolId, Vec<Span>>;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["unknown"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Validation reads grouped per (enclosing function, checked binding).
        let mut groups: FxHashMap<(NodeId, SymbolId), Vec<Read>> = FxHashMap::default();
        let mut narrowings: Vec<(SymbolId, Span)> = Vec::new();
        for node in semantic.nodes().iter() {
            if let Some(narrowing) = concrete_type_narrowing(node.kind(), semantic) {
                narrowings.push(narrowing);
            }
            let AstKind::BinaryExpression(bin) = node.kind() else { continue };
            let Some((ident, kind)) = classify(bin) else { continue };
            let Some(symbol) = resolved_symbol(ident, semantic) else { continue };
            if !symbol_is_annotated_unknown(symbol, semantic) {
                continue;
            }
            let function = enclosing_function_id(node.id(), semantic);
            groups.entry((function, symbol)).or_default().push(Read { kind, span: bin.span });
        }
        if groups.is_empty() {
            return Vec::new();
        }
        let narrowed = narrowed_branches(semantic, &narrowings);

        let mut fired: FxHashMap<NodeId, u32> = FxHashMap::default();
        for ((function, symbol), reads) in &groups {
            let branches = narrowed.get(symbol).map_or(&[][..], Vec::as_slice);
            let Some(first) = first_shape_probe(reads, branches) else { continue };
            fired
                .entry(*function)
                .and_modify(|start| *start = (*start).min(first))
                .or_insert(first);
        }

        let mut diagnostics: Vec<Diagnostic> = fired
            .into_values()
            .map(|span_start| {
                let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Hand-rolled shape validation of an `unknown` value — parse \
                              it once with a schema validator (zod, valibot, …) instead \
                              of checking properties by hand."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                }
            })
            .collect();
        diagnostics.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
        diagnostics
    }
}

/// Offset of the first hand-rolled shape check among `reads`, skipping the ones
/// inside a branch where the binding carries a concrete type. `None` when what
/// is left is legitimate narrowing rather than validation: no property-level
/// check, and fewer than two literal comparisons on the binding itself.
fn first_shape_probe(reads: &[Read], narrowed: &[Span]) -> Option<u32> {
    let probes: Vec<&Read> = reads
        .iter()
        .filter(|read| !narrowed.iter().any(|branch| span_contains(*branch, read.span)))
        .collect();
    let member_count = probes.iter().filter(|read| matches!(read.kind, ReadKind::Member)).count();
    if member_count == 0 && probes.len() < 2 {
        return None;
    }
    probes.iter().map(|read| read.span.start).min()
}

/// A narrowing that gives a binding a concrete type, as the binding and the
/// span of the test: `v instanceof C`, or `Array.isArray(v)`. An object-ness
/// gate narrows to an unnamed bag whose properties stay `unknown`, so it is
/// not one — probing those is what the rule reports.
fn concrete_type_narrowing(
    kind: AstKind,
    semantic: &oxc_semantic::Semantic,
) -> Option<(SymbolId, Span)> {
    match kind {
        AstKind::BinaryExpression(bin) if bin.operator == BinaryOperator::Instanceof => {
            let Expression::Identifier(ident) = &bin.left else { return None };
            Some((resolved_symbol(ident, semantic)?, bin.span))
        }
        AstKind::CallExpression(call) => {
            let Expression::StaticMemberExpression(callee) = &call.callee else { return None };
            if callee.property.name.as_str() != "isArray" {
                return None;
            }
            let Expression::Identifier(global) = &callee.object else { return None };
            if global.name.as_str() != "Array" || !identifier_is_unshadowed_global(global, semantic)
            {
                return None;
            }
            let [Argument::Identifier(subject)] = call.arguments.as_slice() else { return None };
            Some((resolved_symbol(subject, semantic)?, call.span))
        }
        _ => None,
    }
}

/// Map each narrowed binding to the branch spans its narrowings guard: the
/// consequent of an `if`/ternary whose test asserts one, and the right operand
/// of an `&&` whose left does. A read there is a read of the narrowed type.
fn narrowed_branches(
    semantic: &oxc_semantic::Semantic,
    narrowings: &[(SymbolId, Span)],
) -> NarrowedBranches {
    let mut branches = NarrowedBranches::default();
    if narrowings.is_empty() {
        return branches;
    }
    for node in semantic.nodes().iter() {
        let (test, guarded) = match node.kind() {
            AstKind::IfStatement(stmt) => (&stmt.test, stmt.consequent.span()),
            AstKind::ConditionalExpression(cond) => (&cond.test, cond.consequent.span()),
            AstKind::LogicalExpression(logical) if logical.operator == LogicalOperator::And => {
                (&logical.left, logical.right.span())
            }
            _ => continue,
        };
        for (symbol, narrowing) in narrowings {
            if asserts_narrowing(test, *narrowing) {
                branches.entry(*symbol).or_default().push(guarded);
            }
        }
    }
    branches
}

/// True when `test` holds only if the narrowing at `span` held: the test is
/// that narrowing, or an `&&` chain one of whose operands is. A negation or an
/// `||` breaks the implication and stops the walk.
fn asserts_narrowing(test: &Expression, span: Span) -> bool {
    match test {
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::And => {
            asserts_narrowing(&logical.left, span) || asserts_narrowing(&logical.right, span)
        }
        Expression::ParenthesizedExpression(paren) => asserts_narrowing(&paren.expression, span),
        other => other.span() == span,
    }
}

/// Classify a binary expression as a validation read, returning the checked
/// base identifier. `typeof v === '...'` on the bare binding never counts: a
/// lone primitive narrow and the object-ness gate are legitimate narrowing.
fn classify<'a>(bin: &'a BinaryExpression<'a>) -> Option<(&'a IdentifierReference<'a>, ReadKind)> {
    if bin.operator == BinaryOperator::In {
        let Expression::StringLiteral(_) = &bin.left else { return None };
        let Expression::Identifier(ident) = &bin.right else { return None };
        return Some((ident, ReadKind::Member));
    }
    if !matches!(
        bin.operator,
        BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::Equality
            | BinaryOperator::Inequality
    ) {
        return None;
    }
    for (side, other) in [(&bin.left, &bin.right), (&bin.right, &bin.left)] {
        match side {
            // `typeof v.prop === '<shape primitive>'`.
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Typeof => {
                let Expression::StaticMemberExpression(member) = &unary.argument else {
                    continue;
                };
                let Expression::Identifier(ident) = &member.object else { continue };
                let Expression::StringLiteral(lit) = other else { continue };
                if SHAPE_PRIMITIVES.contains(&lit.value.as_str()) {
                    return Some((ident, ReadKind::Member));
                }
            }
            // `v.prop === <literal>` — a discriminant or field check.
            Expression::StaticMemberExpression(member) => {
                let Expression::Identifier(ident) = &member.object else { continue };
                if is_shape_literal(other) {
                    return Some((ident, ReadKind::Member));
                }
            }
            // `v === <literal>` — one leg of a hand-rolled literal union.
            Expression::Identifier(ident) => {
                if is_shape_literal(other) {
                    return Some((ident, ReadKind::BaseLiteral));
                }
            }
            _ => {}
        }
    }
    None
}

/// Literals a schema would model. `null`/`undefined` are excluded: comparing
/// against them is presence checking, not shape validation.
fn is_shape_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_) | Expression::NumericLiteral(_) | Expression::BooleanLiteral(_)
    )
}

fn resolved_symbol(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> Option<SymbolId> {
    let reference_id = ident.reference_id.get()?;
    semantic.scoping().get_reference(reference_id).symbol_id()
}

/// True when the symbol's declaration carries an explicit `: unknown`
/// annotation — a parameter or variable the author declared as unvalidated
/// boundary data. The walk stops at the first declaration-shaped ancestor so a
/// binding nested in another declaration (a parameter inside an initializer,
/// a catch binding) cannot inherit an outer annotation.
fn symbol_is_annotated_unknown(symbol: SymbolId, semantic: &oxc_semantic::Semantic) -> bool {
    let declaration_id = semantic.scoping().symbol_declaration(symbol);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(declaration_id)).chain(nodes.ancestor_kinds(declaration_id))
    {
        match kind {
            AstKind::FormalParameter(parameter) => {
                return annotation_is_unknown(parameter.type_annotation.as_deref());
            }
            AstKind::VariableDeclarator(declarator) => {
                return annotation_is_unknown(declarator.type_annotation.as_deref());
            }
            AstKind::CatchClause(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

fn annotation_is_unknown(annotation: Option<&oxc_ast::ast::TSTypeAnnotation>) -> bool {
    annotation.is_some_and(|annotation| {
        matches!(annotation.type_annotation, TSType::TSUnknownKeyword(_))
    })
}

/// The node id of the nearest enclosing function, or the program for
/// top-level code — the granularity of one diagnostic.
fn enclosing_function_id(node_id: NodeId, semantic: &oxc_semantic::Semantic) -> NodeId {
    let nodes = semantic.nodes();
    let mut current = node_id;
    loop {
        let parent = nodes.parent_id(current);
        if parent == current {
            return current;
        }
        if matches!(
            nodes.kind(parent),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_)
        ) {
            return parent;
        }
        current = parent;
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

    // The natalia-v2 widget shapes the rule is built for (issue #8091).

    #[test]
    fn flags_guard_call_plus_typeof_on_unknown_param() {
        let d = run_on(
            "function parseSessionToken(value: unknown): string | null {\n  \
             if (isRecord(value) && typeof value.session_token === 'string') {\n    \
             return value.session_token;\n  }\n  return null;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_early_return_literal_comparisons() {
        let d = run_on(
            "function parseHistoryEntry(value: unknown) {\n  \
             if (!isRecord(value)) { return null; }\n  \
             if (value.sender !== 'visitor' && value.sender !== 'agent') { return null; }\n  \
             return value;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_in_check_on_unknown_annotated_local() {
        let d = run_on(
            "async function fetchEnvelope(response: Response) {\n  \
             const payload: unknown = await response.json();\n  \
             if (!isRecord(payload) || !('data' in payload)) { return null; }\n  \
             return payload.data;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // A type predicate over an `unknown` param is a hand-written schema; the
    // predicate form is no exemption here, unlike no-typeof-prefer-schema.
    #[test]
    fn flags_type_predicate_guard_over_unknown_param() {
        let d = run_on(
            "function isChoiceOption(value: unknown): value is ChoiceOption {\n  \
             return isRecord(value) && typeof value.id === 'string' \
             && typeof value.label === 'string';\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_literal_union_guard() {
        let d = run_on(
            "function isMediaKind(value: unknown): value is MediaKind {\n  \
             return value === 'image' || value === 'video' \
             || value === 'document' || value === 'audio';\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_discriminated_parser_once() {
        let d = run_on(
            "function parseBubble(value: unknown) {\n  \
             if (!isRecord(value)) { return null; }\n  \
             if (value.type === 'text') {\n    \
             return typeof value.text === 'string' ? { text: value.text } : null;\n  }\n  \
             if (value.type === 'media') {\n    \
             if (typeof value.url !== 'string') { return null; }\n    \
             return { url: value.url };\n  }\n  return null;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_each_function_separately() {
        let d = run_on(
            "function isA(value: unknown) {\n  return isRecord(value) && \
             typeof value.a === 'string';\n}\n\
             function isB(value: unknown) {\n  return isRecord(value) && \
             typeof value.b === 'string';\n}",
        );
        assert_eq!(d.len(), 2, "{d:?}");
    }

    // The object-ness gate every guard delegates to — narrowing, not shape.
    #[test]
    fn allows_record_gate_utility() {
        let d = run_on(
            "function isRecord(value: unknown): value is Record<string, unknown> {\n  \
             return typeof value === 'object' && value !== null;\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_lone_typeof_narrowing() {
        let d = run_on(
            "function label(value: unknown): string {\n  \
             return typeof value === 'string' ? value : '';\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_single_base_literal_sentinel() {
        let d = run_on(
            "function isStop(value: unknown): boolean {\n  return value === 'stop';\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Issue #6097 regression shape: no `unknown` annotation, no finding.
    #[test]
    fn allows_shape_check_on_untyped_param() {
        let d = run_on(
            "function nameFromSchema(schema) {\n  \
             if (typeof schema === 'object' && typeof schema.title === 'string') {\n    \
             return schema.title;\n  }\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_discriminant_dispatch_on_typed_param() {
        let d = run_on(
            "function render(message: WidgetMessage) {\n  \
             if (message.type === 'close') { return null; }\n  return message;\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Comparing a property to another binding proves nothing about shape.
    #[test]
    fn allows_member_compared_to_identifier() {
        let d = run_on(
            "function matches(value: unknown, expected: string) {\n  \
             return isRecord(value) && value.type === expected;\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // A catch binding is `unknown` by contract, not by a schema gap.
    #[test]
    fn allows_catch_binding_probing() {
        let d = run_on(
            "function run() {\n  try { work(); } catch (error: unknown) {\n    \
             if (isRecord(error) && error.code === 'ENOENT') { return null; }\n  }\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Issue #8501: `Array.isArray` narrows to a real array, so `.length` is a
    // typed read, not a probe into an unvalidated bag.
    #[test]
    fn allows_length_read_in_array_isarray_branch() {
        let d = run_on(
            "function filtersFromSearch(search: object, keys: readonly string[]) {\n  \
             return keys.map((key) => {\n    \
             const rawValue: unknown = search[key];\n    \
             if (Array.isArray(rawValue) && rawValue.every((item) => typeof item === 'string')) \
             {\n      return rawValue.length === 0 ? null : rawValue;\n    }\n    \
             return null;\n  });\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_property_read_in_instanceof_branch() {
        let d = run_on(
            "function toTransportError(cause: unknown): Error {\n  \
             if (cause instanceof DOMException && cause.name === 'AbortError') {\n    \
             return cause;\n  }\n  return new Error('transport');\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_property_read_in_nested_instanceof_branch() {
        let d = run_on(
            "function toTransportError(cause: unknown): Error {\n  \
             if (cause instanceof DOMException) {\n    \
             if (cause.name === 'AbortError') { return cause; }\n  }\n  \
             return new Error('transport');\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_property_read_in_instanceof_ternary_branch() {
        let d = run_on(
            "function statusOf(value: unknown): number {\n  \
             return value instanceof Response ? (value.status === 204 ? 0 : 1) : -1;\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // The exemption is the guarded branch, not the whole function: a rejection
    // guard leaves the hand-rolled validation after it in scope.
    #[test]
    fn flags_shape_check_outside_the_narrowed_branch() {
        let d = run_on(
            "function parse(value: unknown) {\n  \
             if (Array.isArray(value)) { return null; }\n  \
             if (isRecord(value) && typeof value.id === 'string') { return value; }\n  \
             return null;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // A negated narrowing asserts nothing inside the branch it guards.
    #[test]
    fn flags_shape_check_in_negated_narrowing_branch() {
        let d = run_on(
            "function parse(value: unknown) {\n  \
             if (!Array.isArray(value)) {\n    \
             return isRecord(value) && typeof value.id === 'string';\n  }\n  \
             return false;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // A shadowed `Array` says nothing about the built-in guard.
    #[test]
    fn flags_shape_check_under_shadowed_array_global() {
        let d = run_on(
            "function parse(Array: { isArray(v: unknown): boolean }, value: unknown) {\n  \
             if (Array.isArray(value) && typeof value.id === 'string') { return value; }\n  \
             return null;\n}",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn allows_unknown_param_without_shape_reads() {
        let d = run_on(
            "function serialize(value: unknown): string {\n  \
             return JSON.stringify(value);\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }
}
