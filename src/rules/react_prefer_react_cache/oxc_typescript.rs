//! OxcCheck backend for react-prefer-react-cache.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn body_has_await_or_fetch(source: &str, span: oxc_span::Span) -> bool {
    let text = &source[span.start as usize..span.end as usize];
    text.contains("await ") || text.contains("fetch(")
}

fn is_cache_wrapper(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "cache",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React" && member.property.name.as_str() == "cache"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn emit(
    name: &str,
    span: oxc_span::Span,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Exported async fetcher `{name}` should be wrapped in \
             `React.cache(...)` so multiple Server Components in the \
             same render share one request."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration, AstType::ExportDefaultDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Only flag in React/Next projects.
        let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
            return;
        };
        if !pkg.has_dep_or_engine("react") && !pkg.has_dep_or_engine("next") {
            return;
        }

        // Only fire in files that could be React Server Components.
        // Backend .ts files (Elysia, Drizzle, etc.) sharing the same
        // package.json shouldn't get this rule.
        let trimmed = ctx.source.trim_start();
        let has_directive = trimmed.starts_with("'use server'")
            || trimmed.starts_with("\"use server\"")
            || trimmed.starts_with("'use client'")
            || trimmed.starts_with("\"use client\"");
        let is_rsc_candidate = ctx.file.path_segments.in_app_router
            || ctx.file.path_segments.in_pages_router
            || matches!(ctx.lang, crate::files::Language::Tsx)
            || has_directive;
        if !is_rsc_candidate {
            return;
        }

        // Only flag at module scope
        let nodes = semantic.nodes();
        if let Some(parent) = nodes.ancestors(node.id()).nth(1)
            && !matches!(parent.kind(), AstKind::Program(_)) {
                return;
            }

        match node.kind() {
            AstKind::ExportNamedDeclaration(export) => {
                let Some(decl) = &export.declaration else { return };
                match decl {
                    oxc_ast::ast::Declaration::FunctionDeclaration(func) => {
                        if !func.r#async {
                            return;
                        }
                        let Some(id) = &func.id else { return };
                        let name = id.name.as_str();
                        if starts_with_uppercase(name) {
                            return;
                        }
                        if !body_has_await_or_fetch(ctx.source, func.span()) {
                            return;
                        }
                        emit(name, id.span, ctx, diagnostics);
                    }
                    oxc_ast::ast::Declaration::VariableDeclaration(var_decl) => {
                        for declarator in &var_decl.declarations {
                            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                                &declarator.id
                            else {
                                continue;
                            };
                            let name = id.name.as_str();
                            let Some(init) = &declarator.init else { continue };

                            // Skip if already wrapped in cache(...)
                            if is_cache_wrapper(init) {
                                continue;
                            }

                            let is_async_fn = match init {
                                Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
                                Expression::FunctionExpression(func) => func.r#async,
                                _ => false,
                            };
                            if !is_async_fn {
                                continue;
                            }
                            if starts_with_uppercase(name) {
                                continue;
                            }
                            if !body_has_await_or_fetch(ctx.source, init.span()) {
                                continue;
                            }
                            emit(name, id.span, ctx, diagnostics);
                        }
                    }
                    _ => {}
                }
            }
            AstKind::ExportDefaultDeclaration(export) => {
                if let oxc_ast::ast::ExportDefaultDeclarationKind::FunctionDeclaration(func) =
                    &export.declaration
                {
                    if !func.r#async {
                        return;
                    }
                    let Some(id) = &func.id else { return };
                    let name = id.name.as_str();
                    if starts_with_uppercase(name) {
                        return;
                    }
                    if !body_has_await_or_fetch(ctx.source, func.span()) {
                        return;
                    }
                    emit(name, id.span, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}
