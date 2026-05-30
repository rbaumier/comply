//! elysia-onerror-before-plugin — OXC backend.
//! Flag `.onError(...)` chained after `.use(plugin)` in the same Elysia chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Collect method names from a chained call expression, innermost first.
/// E.g. `new Elysia().use(p).onError(h)` -> ["use", "onError"]
fn collect_chain<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
) -> Vec<(&'a str, u32)> {
    let mut methods = Vec::new();
    let mut current_callee = &call.callee;

    // The outermost call is the node we start at — walk inward via callee.
    loop {
        let Expression::StaticMemberExpression(member) = current_callee else {
            break;
        };
        let name = member.property.name.as_str();
        let span_start = member.span.start;
        methods.push((name, span_start));

        // The object of this member is the inner call.
        let Expression::CallExpression(inner_call) = &member.object else {
            break;
        };
        current_callee = &inner_call.callee;
    }

    methods.reverse();
    methods
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onError"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !ctx.project.has_framework("elysia") {
            return;
        }

        // Only operate on the outermost call in a chain: if our parent is a
        // StaticMemberExpression whose object is this node, we're not outermost.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::StaticMemberExpression(m) = parent.kind()
            && let Expression::CallExpression(parent_call_obj) = &m.object
                && parent_call_obj.span == call.span {
                    return;
                }

        let methods = collect_chain(call);
        if methods.len() < 2 {
            return;
        }

        let mut seen_use = false;
        for (name, span_start) in &methods {
            if *name == "use" {
                seen_use = true;
                continue;
            }
            if seen_use && *name == "onError" {
                let (line, column) = byte_offset_to_line_col(ctx.source, *span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`.onError(...)` chained after `.use(plugin)` won't catch errors thrown by that plugin — move it before `.use(...)`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_onerror_after_use() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().use(plugin).onError(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_onerror_before_use() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onError(() => {}).use(plugin);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_onerror_alone() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onError(() => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "new Elysia().use(plugin).onError(() => {});";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn ignores_use_mutation_optional_onerror_forwarding() {
        // Regression for #377: a useMutation wrapper that forwards the
        // caller's optional `onError` callback must not be flagged.
        let src = "import { useMutation } from '@tanstack/react-query';\n\
            export function useFormMutation(options) {\n\
              return useMutation({\n\
                onError: (error, variables, context, mutation) => {\n\
                  options.onError?.(error, variables, context, mutation);\n\
                },\n\
              });\n\
            }";
        assert!(run_on(src).is_empty());
    }
}
