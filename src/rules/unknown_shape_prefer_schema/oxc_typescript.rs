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

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryExpression, BinaryOperator, Expression, IdentifierReference, TSType, UnaryOperator,
};
use oxc_semantic::{NodeId, SymbolId};
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
    span_start: u32,
}

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
        for node in semantic.nodes().iter() {
            let AstKind::BinaryExpression(bin) = node.kind() else { continue };
            let Some((ident, kind)) = classify(bin) else { continue };
            let Some(symbol) = resolved_symbol(ident, semantic) else { continue };
            if !symbol_is_annotated_unknown(symbol, semantic) {
                continue;
            }
            let function = enclosing_function_id(node.id(), semantic);
            groups
                .entry((function, symbol))
                .or_default()
                .push(Read { kind, span_start: bin.span.start });
        }

        let mut fired: FxHashMap<NodeId, u32> = FxHashMap::default();
        for ((function, _), reads) in &groups {
            let member_count =
                reads.iter().filter(|read| matches!(read.kind, ReadKind::Member)).count();
            if member_count == 0 && reads.len() < 2 {
                continue;
            }
            let first = reads.iter().map(|read| read.span_start).min().unwrap_or(0);
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

    #[test]
    fn allows_unknown_param_without_shape_reads() {
        let d = run_on(
            "function serialize(value: unknown): string {\n  \
             return JSON.stringify(value);\n}",
        );
        assert!(d.is_empty(), "{d:?}");
    }
}
