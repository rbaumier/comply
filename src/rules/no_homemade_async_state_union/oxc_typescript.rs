//! no-homemade-async-state-union oxc backend.
//!
//! Node-dispatched rather than whole-file: the union half hangs off
//! `TSUnionType`, so a `type` alias, a `useState<…>()` type argument and a
//! `status:` field all reach it through the same node, and the object half
//! hangs off `TSTypeLiteral` / `TSInterfaceDeclaration` / the `useState` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::async_state_helpers as vocabulary;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, CallExpression, Expression, ObjectExpression, ObjectPropertyKind, TSSignature,
    TSType, TSUnionType,
};
use oxc_semantic::{AstNode, NodeId, Semantic};
use std::sync::Arc;

pub struct Check;

/// Two async words make a state machine; one is a word the domain also uses.
const MIN_ASYNC_MEMBERS: usize = 2;

/// The fix, appended to every message. Kept as one string so the union half
/// and the object half cannot drift apart on the advice they give.
const REMEDIATION: &str = "read `status` / `isPending` / `error` from the query result \
                           (TanStack Query, SWR) or return a `Result`; no parallel state machine";

/// Where a flagged union sits, which decides both how the message names it and
/// whether `react-no-request-state-mirror` owns the diagnostic instead.
enum UnionSite {
    TypeAlias(String),
    Property(String),
    Other,
}

fn make_diagnostic(ctx: &CheckCtx, offset: u32, message: String) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Error,
        span: None,
    }
}

/// True when this union is a member of an enclosing union: `("idle" |
/// "loading") | null` is one type, and only its outermost node reports it.
fn is_nested_union(semantic: &Semantic, node_id: NodeId) -> bool {
    for kind in semantic.nodes().ancestor_kinds(node_id) {
        match kind {
            AstKind::TSParenthesizedType(_) => {}
            AstKind::TSUnionType(_) => return true,
            _ => return false,
        }
    }
    false
}

/// Name the declaration the union annotates, skipping the wrapper nodes
/// (parentheses, the `: ` annotation) that carry no name of their own.
fn union_site(semantic: &Semantic, node_id: NodeId) -> UnionSite {
    for kind in semantic.nodes().ancestor_kinds(node_id) {
        match kind {
            AstKind::TSParenthesizedType(_) | AstKind::TSTypeAnnotation(_) => {}
            AstKind::TSTypeAliasDeclaration(alias) => {
                return UnionSite::TypeAlias(alias.id.name.to_string());
            }
            AstKind::TSPropertySignature(property) => {
                return property.key.static_name().map_or(UnionSite::Other, |name| {
                    UnionSite::Property(name.to_string())
                });
            }
            _ => return UnionSite::Other,
        }
    }
    UnionSite::Other
}

/// `type Status = "idle" | "loading"`, `useState<"loading" | "error">()`,
/// `status: "loading" | "error"` — one shape, three places to write it.
fn homemade_union(
    union: &TSUnionType,
    node_id: NodeId,
    semantic: &Semantic,
    ctx: &CheckCtx,
) -> Option<Diagnostic> {
    if is_nested_union(semantic, node_id) {
        return None;
    }
    let mut literals = Vec::new();
    for member in &union.types {
        vocabulary::collect_string_literals(member, &mut literals);
    }
    // Cheap structural bail before any config read: most unions in a codebase
    // hold no string literal at all.
    if literals.len() < MIN_ASYNC_MEMBERS {
        return None;
    }

    let async_words = vocabulary::async_literals(ctx);
    let matched: Vec<&str> = literals
        .into_iter()
        .filter(|value| vocabulary::matches(value, &async_words))
        .collect();
    if matched.len() < MIN_ASYNC_MEMBERS {
        return None;
    }
    let purely_async = vocabulary::async_only_literals(ctx);
    if !matched
        .iter()
        .any(|value| vocabulary::matches(value, &purely_async))
    {
        return None;
    }

    let site = union_site(semantic, node_id);
    if matches!(site, UnionSite::TypeAlias(_)) && vocabulary::imports_query_module(semantic) {
        return None;
    }

    let members = matched
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(" | ");
    let subject = match site {
        UnionSite::TypeAlias(name) | UnionSite::Property(name) => {
            format!("`{name}` is homemade async state")
        }
        UnionSite::Other => "homemade async state".to_string(),
    };
    Some(make_diagnostic(
        ctx,
        union.span.start,
        format!("{subject} (`{members}`) — {REMEDIATION}."),
    ))
}

/// `boolean`, and `boolean | undefined` — the shapes a loading flag is written
/// in. A `loading: "idle" | "done"` field is a union, not a flag.
fn is_boolean_type(ty: &TSType) -> bool {
    match ty {
        TSType::TSBooleanKeyword(_) => true,
        TSType::TSParenthesizedType(paren) => is_boolean_type(&paren.type_annotation),
        TSType::TSUnionType(union) => union.types.iter().any(is_boolean_type),
        _ => false,
    }
}

/// The async field names an object carries, plus what tells a state machine
/// apart from a bag of rendering inputs. See [`AsyncFields::is_state_machine`].
struct AsyncFields {
    known: Vec<String>,
    flag_names: Vec<String>,
    error_names: Vec<String>,
    min_ratio: f64,
    names: Vec<String>,
    /// Every member of the object, async or not — the denominator of the
    /// density test.
    total_members: usize,
    /// A boolean `loading` / `isLoading`. Without it `{ data, error }` is a
    /// `Result` in disguise — the shape the rule asks for.
    has_boolean_flag: bool,
    /// An `error` / `isError` channel. Without it `{ data, isLoading }` is a
    /// presentational component's props, fed by a query it does not own and
    /// with nowhere to report a failure.
    has_error_channel: bool,
}

impl AsyncFields {
    /// Reads the vocabulary once, so a wide object type does not re-read it
    /// per field. `total_members` is the object's full member count, methods
    /// and spreads included — a 25-prop component must not look dense because
    /// only two of its members carry a name this rule can read.
    fn new(ctx: &CheckCtx, total_members: usize) -> Self {
        Self {
            known: vocabulary::async_fields(ctx),
            flag_names: vocabulary::async_only_fields(ctx),
            error_names: vocabulary::async_error_fields(ctx),
            min_ratio: vocabulary::min_async_field_ratio(ctx),
            names: Vec::new(),
            total_members,
            has_boolean_flag: false,
            has_error_channel: false,
        }
    }

    /// Record `name` when it belongs to the vocabulary. `annotates_boolean`
    /// answers "is this field a boolean here?" for the caller's own syntax —
    /// a type annotation on a signature, a literal on an object property.
    fn record(&mut self, name: &str, annotates_boolean: bool) {
        if !vocabulary::matches(name, &self.known) {
            return;
        }
        if annotates_boolean && vocabulary::matches(name, &self.flag_names) {
            self.has_boolean_flag = true;
        }
        if vocabulary::matches(name, &self.error_names) {
            self.has_error_channel = true;
        }
        self.names.push(name.to_string());
    }

    /// The object is the state, not an object that happens to hold a spinner
    /// flag: enough async fields, both discriminants, and async fields making
    /// up at least `min_ratio` of the whole. A snackbar model and a 25-prop
    /// table component both carry `loading` and `error` and neither is a
    /// request state machine.
    fn is_state_machine(&self) -> bool {
        self.names.len() >= MIN_ASYNC_MEMBERS
            && self.has_boolean_flag
            && self.has_error_channel
            && self.names.len() as f64 >= self.total_members as f64 * self.min_ratio
    }
}

/// The `{ data, loading, error }` triplet as a type.
fn homemade_object_type(
    members: &[TSSignature],
    offset: u32,
    declared_name: Option<&str>,
    ctx: &CheckCtx,
) -> Option<Diagnostic> {
    if members.len() < MIN_ASYNC_MEMBERS {
        return None;
    }
    let mut fields = AsyncFields::new(ctx, members.len());
    for member in members {
        let TSSignature::TSPropertySignature(property) = member else {
            continue;
        };
        let Some(name) = property.key.static_name() else {
            continue;
        };
        let annotates_boolean = property
            .type_annotation
            .as_ref()
            .is_some_and(|annotation| is_boolean_type(&annotation.type_annotation));
        fields.record(&name, annotates_boolean);
    }
    build_object_diagnostic(&fields, offset, declared_name, ctx)
}

/// `useState({ data: null, loading: false, error: null })` — the same triplet,
/// held in one piece of component state instead of a declared type.
fn homemade_use_state_object(call: &CallExpression, ctx: &CheckCtx) -> Option<Diagnostic> {
    if !vocabulary::is_use_state_call(call) {
        return None;
    }
    let Some(Argument::ObjectExpression(initial)) = call.arguments.first() else {
        return None;
    };
    let fields = matched_object_fields(initial, ctx);
    build_object_diagnostic(&fields, call.span.start, None, ctx)
}

/// Async field names present in an object literal.
fn matched_object_fields(object: &ObjectExpression, ctx: &CheckCtx) -> AsyncFields {
    let mut fields = AsyncFields::new(ctx, object.properties.len());
    for entry in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = entry else {
            continue;
        };
        let Some(name) = property.key.static_name() else {
            continue;
        };
        let annotates_boolean = matches!(property.value, Expression::BooleanLiteral(_));
        fields.record(&name, annotates_boolean);
    }
    fields
}

/// Shared tail of both object shapes: the state-machine test and the message,
/// so the type half and the `useState` half stay in step.
fn build_object_diagnostic(
    fields: &AsyncFields,
    offset: u32,
    declared_name: Option<&str>,
    ctx: &CheckCtx,
) -> Option<Diagnostic> {
    if !fields.is_state_machine() {
        return None;
    }
    let subject = declared_name.map_or_else(
        || "homemade async state".to_string(),
        |name| format!("`{name}` is homemade async state"),
    );
    Some(make_diagnostic(
        ctx,
        offset,
        format!(
            "{subject} (`{{ {} }}`) — {REMEDIATION}.",
            fields.names.join(", ")
        ),
    ))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::TSUnionType,
            AstType::TSTypeLiteral,
            AstType::TSInterfaceDeclaration,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let found = match node.kind() {
            AstKind::TSUnionType(union) => homemade_union(union, node.id(), semantic, ctx),
            AstKind::TSTypeLiteral(literal) => {
                homemade_object_type(&literal.members, literal.span.start, None, ctx)
            }
            AstKind::TSInterfaceDeclaration(declaration) => homemade_object_type(
                &declaration.body.body,
                declaration.span.start,
                Some(declaration.id.name.as_str()),
                ctx,
            ),
            AstKind::CallExpression(call) => homemade_use_state_object(call, ctx),
            _ => None,
        };
        diagnostics.extend(found);
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
    use super::Check;

    fn run(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    // ── The homemade union ─────────────────────────────────────────────

    #[test]
    fn flags_a_hand_rolled_status_union() {
        let diagnostics = run(r#"type Status = "idle" | "loading" | "error";"#);
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one diagnostic, got {diagnostics:?}"
        );
        assert!(
            diagnostics[0].message.contains("`Status`"),
            "message should name the alias, got {:?}",
            diagnostics[0].message
        );
    }

    #[test]
    fn flags_a_use_state_type_argument_union() {
        let src = r#"
            import { useState } from "react";
            function C() {
                const [phase, setPhase] = useState<"loading" | "error">("loading");
                return phase;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_a_status_field_union() {
        let diagnostics = run(r#"interface Panel { status: "loading" | "error"; }"#);
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one diagnostic, got {diagnostics:?}"
        );
        assert!(
            diagnostics[0].message.contains("`status`"),
            "message should name the field, got {:?}",
            diagnostics[0].message
        );
    }

    #[test]
    fn flags_a_nested_union_once() {
        assert_eq!(run(r#"type Phase = ("idle" | "loading") | null;"#).len(), 1);
    }

    #[test]
    fn flags_literals_regardless_of_case() {
        assert_eq!(run(r#"type Phase = "IDLE" | "LOADING";"#).len(), 1);
    }

    #[test]
    fn flags_a_union_in_a_file_with_no_library_import() {
        // The defect does not need TanStack Query to be a defect — SWR, a
        // hand-rolled fetch hook and a `Result` all carry the same state.
        assert_eq!(run(r#"type Fetch = "refetching" | "success";"#).len(), 1);
    }

    // ── The homemade `{ data, loading, error }` object ──────────────────

    #[test]
    fn flags_the_triplet_in_an_interface() {
        let diagnostics = run(
            "interface AsyncState { data: string | null; loading: boolean; error: Error | null; }",
        );
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one diagnostic, got {diagnostics:?}"
        );
        assert!(
            diagnostics[0].message.contains("`AsyncState`"),
            "message should name the interface, got {:?}",
            diagnostics[0].message
        );
    }

    #[test]
    fn flags_the_triplet_in_a_type_literal() {
        assert_eq!(
            run(
                "type AsyncState = { data: string | null; loading: boolean; error: Error | null };"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_the_triplet_in_a_grouped_use_state() {
        let src = r#"
            import { useState } from "react";
            function C() {
                const [state, setState] = useState({ data: null, loading: false, error: null });
                return state.data;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_a_react_namespaced_grouped_use_state() {
        let src = r#"
            import * as React from "react";
            const [state, setState] = React.useState({ data: null, isLoading: false, error: null });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_the_discriminant_pair_without_data() {
        assert_eq!(
            run("interface Q { isLoading: boolean; isError: boolean; }").len(),
            1
        );
    }

    // ── Domain state that shares a word ────────────────────────────────

    #[test]
    fn allows_a_business_state_union() {
        // `pending` names the order, not a request. No purely-async word.
        assert!(run(r#"type OrderState = "pending" | "shipped";"#).is_empty());
    }

    #[test]
    fn allows_a_payment_state_union() {
        assert!(run(r#"type PaymentState = "pending" | "failed";"#).is_empty());
    }

    #[test]
    fn allows_a_single_async_word_in_a_wider_union() {
        assert!(run(r#"type LogLevel = "debug" | "info" | "warn" | "error";"#).is_empty());
    }

    #[test]
    fn allows_an_unrelated_union() {
        assert!(run(r#"type PanelState = "open" | "closed";"#).is_empty());
    }

    #[test]
    fn allows_a_numeric_union() {
        assert!(run("type HttpStatus = 200 | 404 | 500;").is_empty());
    }

    // ── Types that come from the library ───────────────────────────────

    #[test]
    fn allows_an_alias_of_an_imported_status_type() {
        let src = r#"
            import type { QueryStatus } from "@tanstack/react-query";
            type Status = QueryStatus;
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_a_prop_typed_from_the_library() {
        let src = r#"
            import type { QueryStatus } from "@tanstack/react-query";
            export function Spinner({ status }: { status: QueryStatus }) {
                return status === "pending" ? null : null;
            }
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_reading_the_status_the_query_exposes() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            function C() {
                const { status } = useQuery({ queryKey, queryFn });
                return status === "pending" ? null : "done";
            }
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    // ── Shapes that are not a parallel state machine ────────────────────

    #[test]
    fn allows_a_result_shaped_object() {
        // `{ data, error }` with no boolean flag is the shape the rule wants.
        assert!(run("interface Loaded { data: string; error: Error | null; }").is_empty());
    }

    #[test]
    fn allows_presentational_props_with_no_failure_channel() {
        // A generic table receives `data` and `isLoading` from a query it does
        // not own; with nowhere to report a failure it holds no state machine.
        assert!(
            run("type TableProps = { data: readonly string[]; isLoading: boolean; };").is_empty()
        );
    }

    #[test]
    fn allows_a_loading_flag_diluted_in_a_wide_model() {
        // A snackbar carries a spinner flag and an attached error among ten
        // other fields; two async words in a wide object are not the state.
        let source = r#"
            interface SnackbarRaw {
                id?: string;
                persist?: boolean;
                title: string;
                text?: string;
                icon?: string | null;
                closeable?: boolean;
                progress?: number;
                loading?: boolean;
                dialog?: boolean;
                error?: Error;
            }
        "#;
        assert!(run(source).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_a_lone_loading_flag() {
        assert!(run("interface Panel { loading: boolean; title: string; }").is_empty());
    }

    #[test]
    fn allows_a_non_boolean_loading_field() {
        assert!(run(r#"interface Job { loading: string; data: string; }"#).is_empty());
    }

    #[test]
    fn allows_a_use_state_with_an_unrelated_object() {
        let src = r#"
            import { useState } from "react";
            const [form, setForm] = useState({ name: "", email: "" });
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    // ── Territory owned by react-no-request-state-mirror ────────────────

    #[test]
    fn allows_a_type_alias_beside_a_tanstack_import() {
        // `react-no-request-state-mirror` reports this one; two rules on one
        // line would be two diagnostics for a single defect.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type Status = "idle" | "loading";
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn still_flags_a_field_union_beside_a_tanstack_import() {
        // Only the type-alias shape is ceded — the other rule does not see
        // fields, so leaving them out would drop the diagnostic entirely.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            interface Panel { status: "loading" | "error"; }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Files the rule stays out of ────────────────────────────────────

    #[test]
    fn allows_a_union_in_a_test_file() {
        let diagnostics = crate::rules::test_helpers::run_rule_gated(
            &Check,
            r#"type Status = "idle" | "loading";"#,
            "src/thing.test.ts",
        );
        assert!(diagnostics.is_empty(), "expected no diagnostics");
    }
}
