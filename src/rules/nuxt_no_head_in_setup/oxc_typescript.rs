//! nuxt-no-head-in-setup OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useNuxtApp")
        || src.contains("defineComponent")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["head"])
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
        let AstKind::ObjectExpression(obj) = node.kind() else {
            return;
        };

        // Check if this object is inside a defineComponent call
        let mut in_define_component = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::CallExpression(call) = ancestor.kind() {
                if let oxc_ast::ast::Expression::Identifier(id) = &call.callee {
                    if id.name.as_str() == "defineComponent" {
                        in_define_component = true;
                        break;
                    }
                }
            }
        }
        if !in_define_component {
            return;
        }

        // Look for a `head` property or method in this object
        for prop in &obj.properties {
            let span = match prop {
                oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) => {
                    let key_name = match &p.key {
                        oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                        _ => continue,
                    };
                    if key_name != "head" {
                        continue;
                    }
                    p.span
                }
                _ => continue,
            };

            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Use `useHead({ ... })` instead of declaring `head` on component options."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_head_property_in_define_component() {
        let src = "import {} from '#imports';\nexport default defineComponent({ head: { title: 'X' }, setup() { return {}; } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_head_method_in_define_component() {
        let src = "import {} from '#imports';\nexport default defineComponent({ head() { return { title: 'X' }; }, setup() { return {}; } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_use_head_call() {
        let src = "import {} from '#imports';\nexport default defineComponent({ setup() { useHead({ title: 'X' }); return {}; } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_nuxt_files() {
        let src = "export default { head: { title: 'X' } };";
        assert!(run_on(src).is_empty());
    }
}
