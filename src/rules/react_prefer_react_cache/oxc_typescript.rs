//! OxcCheck backend for react-prefer-react-cache.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Return true when the source uses client-only React hooks.
/// A file with useCallback/useMemo calls cannot be a Server Component —
/// these hooks are inert at SSR time and only make sense in client bundles.
fn source_uses_client_hooks(source: &str) -> bool {
    source.contains("useCallback(") || source.contains("useMemo(")
}

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

        // `React.cache(...)` is a server-component primitive — it's a
        // no-op in client components and in non-RSC frameworks
        // (TanStack Start, plain Vite SPA, Remix without server
        // exports). Only fire when we can plausibly tell the file IS a
        // server module.

        // Use pre-scanned directives (handles comments before the directive).
        if ctx.file.directives.use_client {
            return;
        }
        // The project must use Next.js (the only mainstream framework
        // with RSC + React.cache support today). Plain React / TanStack
        // Start / Vite SPA setups don't have the cache mechanism.
        if !pkg.has_dep_or_engine("next") {
            return;
        }

        // A file that calls client-only hooks (useCallback, useMemo) cannot
        // be a Server Component — skip even without an explicit "use client".
        if source_uses_client_hooks(ctx.source) {
            return;
        }

        let is_rsc_candidate = ctx.file.directives.use_server
            || ctx.file.path_segments.in_app_router
            || ctx.file.path_segments.in_pages_router;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::CheckCtx;
    use crate::rules::file_ctx::FileCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Run the OXC check against source in a simulated Next.js app-router
    /// file. Creates a temp dir with `package.json` and the file at
    /// `app/search/page.tsx` so all RSC-candidate signals fire correctly.
    fn run_next_app_router(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"next":"^15","react":"^19"}}"#,
        )
        .unwrap();
        let file_path = dir.path().join("app/search/page.tsx");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::Tsx,
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        let canon_path = canon.as_path();
        let file_ctx = FileCtx::build(canon_path, source, Language::Tsx, &project);

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_full(canon_path, source, &project, &file_ctx);

        let check = Check;
        let kinds = check.interested_kinds();
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty();
            if kinds.contains(&ty) {
                check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn flags_exported_async_fetcher_in_app_router() {
        let src = r#"
export async function fetchSuggestions(query: string) {
    const res = await fetch(`/api/search?q=${query}`);
    return res.json();
}
"#;
        assert_eq!(run_next_app_router(src).len(), 1);
    }

    #[test]
    fn no_fp_use_client_directive() {
        let src = r#""use client";

export async function fetchSuggestions(query: string) {
    const res = await fetch(`/api/search?q=${query}`);
    return res.json();
}
"#;
        assert!(run_next_app_router(src).is_empty());
    }

    // Regression for #381: comment before "use client" was not detected.
    #[test]
    fn no_fp_use_client_with_leading_comment() {
        let src = r#"// SearchCombobox component
"use client";

export async function fetchSuggestions(query: string) {
    const res = await fetch(`/api/search?q=${query}`);
    return res.json();
}
"#;
        assert!(run_next_app_router(src).is_empty());
    }

    // Regression for #381: debounced onSearch callback in client-only component.
    #[test]
    fn no_fp_file_with_usecallback_debounce() {
        let src = r#"
import { debounce } from "lodash";

export async function fetchSuggestions(query: string) {
    const res = await fetch(`/api/search?q=${query}`);
    return res.json();
}

export function SearchCombobox() {
    const debouncedSearch = useCallback(
        debounce(async (query: string) => {
            const results = await fetchSuggestions(query);
            setSuggestions(results);
        }, 300),
        []
    );
    return null;
}
"#;
        assert!(run_next_app_router(src).is_empty());
    }

    #[test]
    fn no_fp_file_with_usememo() {
        let src = r#"
export async function loadData(id: string) {
    const res = await fetch(`/api/data/${id}`);
    return res.json();
}

export function Widget({ id }: { id: string }) {
    const derived = useMemo(() => compute(id), [id]);
    return null;
}
"#;
        assert!(run_next_app_router(src).is_empty());
    }

    #[test]
    fn allows_cache_wrapped_fetcher() {
        let src = r#"import { cache } from "react";

export const getUser = cache(async (id: string) => {
    const res = await fetch(`/api/users/${id}`);
    return res.json();
});
"#;
        assert!(run_next_app_router(src).is_empty());
    }
}
