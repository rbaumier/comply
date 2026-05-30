//! elysia-scope-missing OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const HOOK_METHODS: &[&str] = &[
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "onTransform",
];

fn is_root_app_file(source: &str, path: &std::path::Path) -> bool {
    if source.contains(".listen(") {
        return true;
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        stem,
        "app" | "index" | "server" | "main" | "create-app" | "createApp" | "bootstrap" | "entry"
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !ctx.source.contains("export") {
            return;
        }
        if is_root_app_file(ctx.source, ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop_text = member.property.name.as_str();
        if !HOOK_METHODS.contains(&prop_text) {
            return;
        }

        // Skip optional calls (`options.onError?.()`) — those invoke stored
        // TanStack Query callbacks, not Elysia lifecycle hook registrations.
        if call.optional {
            return;
        }

        // If the file uses any scope marker, skip.
        let s = ctx.source;
        let has_scope = s.contains("as:'global'")
            || s.contains("as: 'global'")
            || s.contains("as:\"global\"")
            || s.contains("as: \"global\"")
            || s.contains("as:'scoped'")
            || s.contains("as: 'scoped'")
            || s.contains("as:\"scoped\"")
            || s.contains("as: \"scoped\"")
            || s.contains(".as('scoped')")
            || s.contains(".as(\"scoped\")")
            || s.contains(".as('global')")
            || s.contains(".as(\"global\")");
        if has_scope {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` in an exported plugin without a scope — hooks default to `local` and won't propagate to the parent app.",
                prop_text
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_elysia_plugin_hook_without_scope() {
        let src = "import { Elysia } from 'elysia';\n\
            export const plugin = new Elysia().onError(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_use_mutation_on_error_object_property() {
        // Regression for #202: `useMutation({ onError: ... })` is a TanStack
        // Query callback, not an Elysia plugin lifecycle hook. The rule must
        // only fire on `.onError(...)` member-call form.
        let src = "import { useMutation } from '@tanstack/react-query';\n\
            export const useFormMutation = () => useMutation({\n\
              onError: (error, variables, context, mutation) => { console.log(error); }\n\
            });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_use_mutation_optional_onerror_forwarding() {
        // Regression for #377: a useMutation wrapper that forwards the
        // caller's optional `onError` callback via `options.onError?.()`
        // must not be flagged — it's a TanStack invocation, not a plugin
        // lifecycle hook registration.
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
