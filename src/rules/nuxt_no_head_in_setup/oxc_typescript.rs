//! nuxt-no-head-in-setup OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, source_contains};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nuxt_source(src: &str) -> bool {
    source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtConfig")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware")
        || source_contains(src, "useNuxtApp")
        || source_contains(src, "defineComponent")
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
        let AstKind::ObjectExpression(obj) = node.kind() else {
            return;
        };
        if !is_nuxt_source(ctx.source) {
            return;
        }

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
