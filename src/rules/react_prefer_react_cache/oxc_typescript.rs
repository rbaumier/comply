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
    crate::oxc_helpers::source_contains(source, "useCallback(") || crate::oxc_helpers::source_contains(source, "useMemo(")
}

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Next.js framework-consumed exports that cannot be wrapped in
/// `React.cache()`: the framework calls them directly with fixed arguments.
/// Pages Router lifecycle (`getServerSideProps`/`getStaticProps`/
/// `getStaticPaths`/`getInitialProps`) and App Router generation APIs
/// (`generateStaticParams`/`generateMetadata`) are not user-callable data
/// fetchers, so deduplication via `cache()` does not apply.
fn is_framework_lifecycle_export(name: &str) -> bool {
    matches!(
        name,
        "getServerSideProps"
            | "getStaticProps"
            | "getStaticPaths"
            | "getInitialProps"
            | "generateStaticParams"
            | "generateMetadata"
    )
}

fn body_has_await_or_fetch(source: &str, span: oxc_span::Span) -> bool {
    let text = &source[span.start as usize..span.end as usize];
    text.contains("await ") || text.contains("fetch(")
}

/// True for a Next.js Pages Router API route file (`pages/api/...`). The
/// route's `handler` export is invoked by the framework with `(req, res)`,
/// not a deduplication-eligible fetcher.
fn is_pages_api_route(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase();
    lower.contains("/pages/api/") || lower.starts_with("pages/api/")
}

/// True for export names the Next.js framework consumes directly and that
/// must not be wrapped in `React.cache()`: lifecycle/generation exports
/// everywhere, plus `handler` inside a `pages/api/` route.
fn is_exempt_export(name: &str, path: &std::path::Path) -> bool {
    is_framework_lifecycle_export(name) || (name == "handler" && is_pages_api_route(path))
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
        // A `'use server'` file is a Server Action module: its exports are
        // mutation invocation endpoints, not deduplication-eligible pure
        // fetchers. Wrapping them in `React.cache()` is semantically wrong
        // (caching a sign-in action would break auth), so exempt the file.
        if ctx.file.directives.use_server {
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

        let is_rsc_candidate = ctx.file.path_segments.in_app_router
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
                        if is_exempt_export(name, ctx.path) {
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
                            if is_exempt_export(name, ctx.path) {
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
                    if is_exempt_export(name, ctx.path) {
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
        run_next_at(source, "app/search/page.tsx")
    }

    /// Like `run_next_app_router` but lets the test place the file at an
    /// arbitrary path relative to the project root (e.g. `pages/index.tsx`,
    /// `pages/api/users.ts`, `app/login/actions.ts`).
    fn run_next_at(source: &str, rel_path: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"next":"^15","react":"^19"}}"#,
        )
        .unwrap();
        let file_path = dir.path().join(rel_path);
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

    // Regression for #1784: a `'use server'` Server Action module is a set of
    // mutation endpoints, not deduplication-eligible fetchers. `signIn`
    // awaits but must not be flagged.
    #[test]
    fn no_fp_use_server_action_module() {
        let src = r#"'use server'

import { redirect } from 'next/navigation'
import { createClient } from '@/lib/supabase/server'

export async function signIn(formData: FormData) {
    const supabase = await createClient()
    const data = {
        email: formData.get('email') as string,
        password: formData.get('password') as string,
    }
    const { error } = await supabase.auth.signInWithPassword(data)
    if (error) redirect('/error')
    redirect('/')
}
"#;
        assert!(run_next_at(src, "app/login/actions.ts").is_empty());
    }

    // Regression for #1784: Pages Router `getServerSideProps` is consumed by
    // the framework and cannot be wrapped in `React.cache()`.
    #[test]
    fn no_fp_pages_router_get_server_side_props() {
        let src = r#"export const getServerSideProps = async (ctx: GetServerSidePropsContext) => {
    const supabase = createServerClient(ctx)
    const { data } = await supabase.from('rooms').select()
    return { props: { rooms: data } }
}
"#;
        assert!(run_next_at(src, "pages/index.tsx").is_empty());
    }

    // Regression for #1784: other Pages Router / App Router framework-consumed
    // exports are also exempt.
    #[test]
    fn no_fp_framework_lifecycle_exports() {
        let src = r#"export async function getStaticProps() {
    const data = await fetch('/api/data')
    return { props: {} }
}

export async function getStaticPaths() {
    const data = await fetch('/api/paths')
    return { paths: [], fallback: false }
}

export async function generateStaticParams() {
    const data = await fetch('/api/params')
    return []
}

export async function generateMetadata() {
    const data = await fetch('/api/meta')
    return {}
}
"#;
        assert!(run_next_at(src, "app/blog/page.tsx").is_empty());
    }

    // Regression for #1784: a `pages/api/` route handler is framework-invoked
    // with `(req, res)`, not a cacheable fetcher.
    #[test]
    fn no_fp_pages_api_handler() {
        let src = r#"export default async function handler(req, res) {
    const data = await fetch('/api/upstream')
    res.json(await data.json())
}
"#;
        assert!(run_next_at(src, "pages/api/users.ts").is_empty());
    }

    // A genuine RSC data fetcher must still be flagged — the exemptions are
    // name/directive/path scoped, not a blanket disable.
    #[test]
    fn still_flags_genuine_fetcher_alongside_exempt_export() {
        let src = r#"export async function getStaticProps() {
    const data = await fetch('/api/data')
    return { props: {} }
}

export async function fetchSuggestions(query: string) {
    const res = await fetch(`/api/search?q=${query}`)
    return res.json()
}
"#;
        assert_eq!(run_next_at(src, "app/search/page.tsx").len(), 1);
    }
}
