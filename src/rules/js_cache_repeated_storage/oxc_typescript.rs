//! js-cache-repeated-storage OxcCheck backend — repeated `localStorage.getItem(key)`
//! / `sessionStorage.getItem(key)` with the same key in one function body.

use rustc_hash::FxHashSet;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

const STORAGE_OBJECTS: &[&str] = &["localStorage", "sessionStorage"];

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage", "sessionStorage"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Get the function body span to scope our search.
        let body_span = match node.kind() {
            AstKind::Function(f) => {
                let Some(body) = &f.body else { return };
                body.span
            }
            AstKind::ArrowFunctionExpression(af) => af.body.span,
            _ => return,
        };

        // Collect all `storage.getItem("key")` calls within this function body.
        let mut calls: Vec<(String, u32)> = Vec::new();

        for child in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = child.kind() else {
                continue;
            };
            // Must be within our function body.
            if call.span.start < body_span.start || call.span.end > body_span.end {
                continue;
            }

            // Skip if inside a nested function.
            let mut in_nested = false;
            for ancestor in semantic.nodes().ancestors(child.id()).skip(1) {
                match ancestor.kind() {
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                        if ancestor.id() != node.id() {
                            in_nested = true;
                        }
                        break;
                    }
                    _ => {}
                }
            }
            if in_nested {
                continue;
            }

            // Check for `localStorage.getItem(...)` / `sessionStorage.getItem(...)`
            let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            if member.property.name.as_str() != "getItem" {
                continue;
            }
            let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
                continue;
            };
            let obj_name = obj.name.as_str();
            if !STORAGE_OBJECTS.contains(&obj_name) {
                continue;
            }

            // Get the first argument (must be a string literal).
            let Some(first_arg) = call.arguments.first() else {
                continue;
            };
            let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else {
                continue;
            };
            let key = lit.value.as_str();
            calls.push((format!("{obj_name}.{key}"), call.span.start));
        }

        let mut seen = FxHashSet::default();
        for (key, span_start) in &calls {
            if !seen.insert(key.clone()) {
                let display_key = key.split('.').next_back().unwrap_or(key);
                let (line, column) = byte_offset_to_line_col(ctx.source, *span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Repeated `getItem(\"{display_key}\")` — read once into a variable.",
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
