//! nuxt-no-setup-outside-definecomponent OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const SETUP_COMPOSABLES: &[&str] = &[
    "useState",
    "useFetch",
    "useAsyncData",
    "useNuxtApp",
    "useRuntimeConfig",
    "useRoute",
    "useRouter",
];

fn is_nuxt_options_api(src: &str) -> bool {
    let nuxt = source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware");
    if !nuxt {
        return false;
    }
    source_contains(src, "export default {") && !source_contains(src, "defineComponent(")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setup"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Get callee name
        let name = match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !SETUP_COMPOSABLES.contains(&name) {
            return;
        }
        if !is_nuxt_options_api(ctx.source) {
            return;
        }

        // Must be at module scope — no function/arrow ancestor
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::FunctionBody(_) => return,
                _ => {}
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}()` called at module scope in an options-API file — wrap in `defineComponent({{ setup() {{ ... }} }})` or use `<script setup>`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
