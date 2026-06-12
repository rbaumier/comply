//! nuxt-no-client-only-in-ssr OxcCheck backend.
//!
//! Flags top-level reads of `window`, `document`, `localStorage`,
//! `sessionStorage`, or `navigator` in a Nuxt file. Allowed only inside
//! any function or behind a `process.client` / `import.meta.client` guard.

use std::sync::Arc;

use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};

pub struct Check;

const BROWSER_GLOBALS: &[&str] = &[
    "window",
    "document",
    "localStorage",
    "sessionStorage",
    "navigator",
];

fn is_nuxt_source(src: &str) -> bool {
    source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtConfig")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware")
        || source_contains(src, "useNuxtApp")
        || source_contains(src, "useRuntimeConfig")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_nuxt_source(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::IdentifierReference(ident) = node.kind() else {
                continue;
            };
            let name = ident.name.as_str();
            if !BROWSER_GLOBALS.contains(&name) {
                continue;
            }

            // Skip if this identifier is a property access (e.g. `foo.window`).
            let parent = semantic.nodes().parent_node(node.id());
            if let AstKind::StaticMemberExpression(member) = parent.kind() {
                // Skip if we are the property, not the object.
                if member.property.span == ident.span {
                    continue;
                }
            }

            // Walk ancestors: if inside a function, skip. If behind process.client guard, skip.
            let mut inside_function = false;
            let mut behind_guard = false;
            let mut depth = 0;
            for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
                match ancestor.kind() {
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                        inside_function = true;
                        break;
                    }
                    AstKind::IfStatement(if_stmt) => {
                        let cond_text = &ctx.source
                            [if_stmt.test.span().start as usize..if_stmt.test.span().end as usize];
                        if cond_text.contains("process.client")
                            || cond_text.contains("import.meta.client")
                        {
                            behind_guard = true;
                            break;
                        }
                    }
                    _ => {}
                }
                depth += 1;
                if depth > 10 {
                    inside_function = true; // bail out safely
                    break;
                }
            }

            if inside_function || behind_guard {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "nuxt-no-client-only-in-ssr".into(),
                message: format!(
                    "`{name}` is browser-only — guard with `if (import.meta.client)` or move into `onMounted`."
                ),
                severity: Severity::Error,
                span: Some((ident.span.start as usize, (ident.span.end - ident.span.start) as usize)),
            });
        }

        diagnostics
    }
}
