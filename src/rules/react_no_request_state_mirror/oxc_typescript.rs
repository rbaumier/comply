//! react-no-request-state-mirror oxc backend.
//!
//! The whole rule is gated on a real `@tanstack/react-query` import: away from
//! the library, `"idle" | "loading"` names a domain state nobody duplicated.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression, TSLiteral, TSType};
use std::sync::Arc;

pub struct Check;

const QUERY_MODULE: &str = "@tanstack/react-query";

/// Literals naming a phase of an in-flight request. `pending` / `error` /
/// `success` are TanStack Query's own `status` values; the others are the words
/// a hand-rolled mirror reaches for.
const REQUEST_STATE_LITERALS: &[&str] = &[
    "idle",
    "loading",
    "pending",
    "error",
    "success",
    "fetching",
    "submitting",
    "saving",
];

/// Two members make the union a request state machine. One is a single word
/// shared with some larger domain enum, which the rule must leave alone.
const MIN_UNION_MEMBERS: usize = 2;

/// True when the file imports `@tanstack/react-query` — a substring match on the
/// source would also accept the module name quoted in a comment.
fn imports_tanstack_query(semantic: &oxc_semantic::Semantic) -> bool {
    semantic.nodes().iter().any(|node| {
        matches!(node.kind(), AstKind::ImportDeclaration(import)
            if import.source.value.as_str() == QUERY_MODULE)
    })
}

fn is_request_state_literal(value: &str, literals: &[&str]) -> bool {
    literals.iter().any(|known| known.eq_ignore_ascii_case(value))
}

/// The built-in phase names plus whatever `extra_literals` adds for the project.
fn configured_literals(extra: &[String]) -> Vec<&str> {
    REQUEST_STATE_LITERALS
        .iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .collect()
}

/// Count the string-literal members of `ty` that name a request phase. The
/// parser keeps parentheses and nested unions as their own nodes, so
/// `("idle" | "loading") | null` has to be walked to count both.
fn count_request_state_members(ty: &TSType, literals: &[&str]) -> usize {
    match ty {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .map(|member| count_request_state_members(member, literals))
            .sum(),
        TSType::TSParenthesizedType(paren) => {
            count_request_state_members(&paren.type_annotation, literals)
        }
        TSType::TSLiteralType(lit) => match &lit.literal {
            TSLiteral::StringLiteral(text) => {
                usize::from(is_request_state_literal(text.value.as_str(), literals))
            }
            _ => 0,
        },
        _ => 0,
    }
}

fn is_use_state_call(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "useState",
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "useState"
                && matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "React")
        }
        _ => false,
    }
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

/// `type Status = "idle" | "loading"` — a second copy of the request lifecycle.
/// Reaching the member threshold already implies a union, so the shape of the
/// annotation needs no separate guard.
fn mirror_in_type_alias(
    alias: &oxc_ast::ast::TSTypeAliasDeclaration,
    ctx: &CheckCtx,
    literals: &[&str],
) -> Option<Diagnostic> {
    if count_request_state_members(&alias.type_annotation, literals) < MIN_UNION_MEMBERS {
        return None;
    }
    let name = alias.id.name.as_str();
    Some(make_diagnostic(
        ctx,
        alias.span.start,
        format!(
            "`{name}` re-models request state as a hand-rolled union — switch on the `status` \
             of the `useQuery` / `useMutation` result instead."
        ),
    ))
}

/// `useState("idle")` — the same lifecycle, held in component state this time.
fn mirror_in_use_state(
    call: &CallExpression,
    ctx: &CheckCtx,
    literals: &[&str],
) -> Option<Diagnostic> {
    if !is_use_state_call(call) {
        return None;
    }
    let Some(Argument::StringLiteral(initial)) = call.arguments.first() else {
        return None;
    };
    let value = initial.value.as_str();
    if !is_request_state_literal(value, literals) {
        return None;
    }
    Some(make_diagnostic(
        ctx,
        call.span.start,
        format!(
            "`useState(\"{value}\")` mirrors request state in component state — read `status` \
             off the `useQuery` / `useMutation` result instead."
        ),
    ))
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[QUERY_MODULE])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !imports_tanstack_query(semantic) {
            return Vec::new();
        }
        let extra = ctx
            .config
            .string_list(super::META.id, "extra_literals", ctx.lang);
        let literals = configured_literals(&extra);
        let flags_use_state = ctx
            .config
            .bool_flag(super::META.id, "check_use_state", ctx.lang);

        semantic
            .nodes()
            .iter()
            .filter_map(|node| match node.kind() {
                AstKind::TSTypeAliasDeclaration(alias) => {
                    mirror_in_type_alias(alias, ctx, &literals)
                }
                AstKind::CallExpression(call) if flags_use_state => {
                    mirror_in_use_state(call, ctx, &literals)
                }
                _ => None,
            })
            .collect()
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

    // ── The mirror the rule exists for ─────────────────────────────────

    #[test]
    fn flags_hand_rolled_status_union() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type RequestState = "idle" | "loading" | "error";
            export function useThing(): RequestState {
                const { data } = useQuery({ queryKey, queryFn });
                return data ? "idle" : "loading";
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
        assert!(
            diags[0].message.contains("RequestState"),
            "message should name the alias, got {:?}",
            diags[0].message
        );
    }

    #[test]
    fn flags_union_of_exactly_two_members() {
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            type SaveState = "saving" | "success";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_state_initialized_with_a_request_phase() {
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            import { useState } from "react";
            function Form() {
                const [state, setState] = useState("idle");
                const { mutate } = useMutation({ mutationFn });
                return state;
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
        assert!(
            diags[0].message.contains("useState(\"idle\")"),
            "message should quote the initial value, got {:?}",
            diags[0].message
        );
    }

    #[test]
    fn flags_react_namespaced_use_state() {
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            import * as React from "react";
            const [status, setStatus] = React.useState("submitting");
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_both_halves_in_one_file() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            import { useState } from "react";
            type Phase = "idle" | "fetching";
            const [phase, setPhase] = useState("idle");
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_nested_union_members() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type Phase = ("idle" | "loading") | null;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_parenthesized_union() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type Phase = ("idle" | "loading");
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_literals_regardless_of_case() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type Phase = "IDLE" | "LOADING";
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // ── No TanStack Query import: the rule stays silent ────────────────

    #[test]
    fn allows_state_union_without_tanstack_import() {
        let src = r#"
            type RequestState = "idle" | "loading" | "error";
            const [state, setState] = useState("idle");
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_module_name_quoted_in_a_comment() {
        // The prefilter passes on the substring; only a real import counts.
        let src = r#"
            // Ported off @tanstack/react-query on purpose.
            type RequestState = "idle" | "loading";
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_import_from_another_module() {
        let src = r#"
            import { useQuery } from "./hooks/use-query";
            type RequestState = "idle" | "loading";
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    // ── Unions and state that describe something other than a request ──

    #[test]
    fn allows_unrelated_domain_union() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type PanelState = "open" | "closed";
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_single_request_word_in_a_wider_union() {
        // `error` is one member of a log-level scale, not a request lifecycle.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type LogLevel = "debug" | "info" | "warn" | "error";
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_numeric_union() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            type HttpStatus = 200 | 404 | 500;
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_use_state_with_unrelated_initial_value() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            import { useState } from "react";
            const [tab, setTab] = useState("overview");
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_use_state_with_no_argument() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            import { useState } from "react";
            const [value, setValue] = useState();
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics");
    }

    #[test]
    fn allows_other_hook_initialized_with_a_request_phase() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            const ref = useRef("idle");
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
}
