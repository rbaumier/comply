//! react-no-request-state-mirror oxc backend.
//!
//! The whole rule is gated on a real `@tanstack/react-query` import: away from
//! the library, `"idle" | "loading"` names a domain state nobody duplicated —
//! `no-homemade-async-state-union` covers that ground with its own guard.
//!
//! The phase vocabulary comes from [`crate::rules::async_state_helpers`], so
//! this rule and `no-homemade-async-state-union` read one list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::async_state_helpers as vocabulary;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression};
use std::sync::Arc;

pub struct Check;

/// Two members make the union a request state machine. One is a single word
/// shared with some larger domain enum, which the rule must leave alone.
const MIN_UNION_MEMBERS: usize = 2;

/// The shared async-state vocabulary plus whatever `extra_literals` adds for
/// the project.
fn configured_literals(ctx: &CheckCtx) -> Vec<String> {
    let mut literals = vocabulary::async_literals(ctx);
    literals.extend(
        ctx.config
            .string_list(super::META.id, "extra_literals", ctx.lang),
    );
    literals
}

/// Count the string-literal members of `ty` that name a request phase.
fn count_request_state_members(ty: &oxc_ast::ast::TSType, literals: &[String]) -> usize {
    let mut members = Vec::new();
    vocabulary::collect_string_literals(ty, &mut members);
    members
        .into_iter()
        .filter(|value| vocabulary::matches(value, literals))
        .count()
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
    literals: &[String],
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
    literals: &[String],
) -> Option<Diagnostic> {
    if !vocabulary::is_use_state_call(call) {
        return None;
    }
    let Some(Argument::StringLiteral(initial)) = call.arguments.first() else {
        return None;
    };
    let value = initial.value.as_str();
    if !vocabulary::matches(value, literals) {
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
        Some(&[vocabulary::QUERY_MODULE])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !vocabulary::imports_query_module(semantic) {
            return Vec::new();
        }
        let literals = configured_literals(ctx);
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
