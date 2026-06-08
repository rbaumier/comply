//! OxcCheck backend for nuxt-no-manual-fetch-in-setup.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useNuxtApp")
        || src.contains("useRuntimeConfig")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nuxt_source(ctx.source) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "fetch" {
            return;
        }

        // Walk parents to check if we're at module scope or inside a `setup` method.
        let mut in_setup = false;
        let mut at_module_scope = true;
        let mut current = node.id();
        let mut depth = 0;
        let nodes = semantic.nodes();
        loop {
            let parent_id = nodes.parent_id(current);
            if parent_id == current {
                break;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::Function(f) => {
                    at_module_scope = false;
                    // Check if this is a setup() method by looking at grandparent
                    // for an ObjectProperty with key "setup".
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::ObjectProperty(prop) = gp.kind() {
                            if prop.key.static_name().is_some_and(|n| n == "setup") {
                                in_setup = true;
                            }
                        }
                    }
                    // Also check if function itself has id "setup"
                    if let Some(id) = &f.id {
                        if id.name.as_str() == "setup" {
                            in_setup = true;
                        }
                    }
                }
                AstKind::ArrowFunctionExpression(_) => {
                    at_module_scope = false;
                }
                _ => {}
            }
            current = parent_id;
            depth += 1;
            if depth > 10 {
                return;
            }
        }

        if !(in_setup || at_module_scope) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `useFetch()` or `useAsyncData()` instead of raw `fetch()` in setup — avoids duplicate SSR + hydration requests.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_fetch_at_module_scope() {
        let src = "import {} from '#imports';\nconst data = await fetch('/api/x').then(r => r.json());";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn flags_fetch_inside_setup_method() {
        let src = "import {} from '#imports';\nexport default defineComponent({ async setup() { const r = await fetch('/api/x'); return {}; } });";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn allows_use_fetch() {
        let src = "import {} from '#imports';\nconst { data } = await useFetch('/api/x');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_nuxt_files() {
        let src = "const data = await fetch('/api/x').then(r => r.json());";
        assert!(run_on(src).is_empty());
    }
}
