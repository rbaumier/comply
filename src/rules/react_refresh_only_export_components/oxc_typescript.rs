//! react-refresh-only-export-components oxc backend for TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Declaration, Expression, ExportDefaultDeclarationKind, ExportNamedDeclaration, Statement,
};
use std::sync::Arc;

pub struct Check;

fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// True for the React custom-hook naming convention: `use` followed by an
/// uppercase letter or digit (`useQueryClient`, `useTheme`, `use2FA`). A bare
/// `use` or `useThing`-lowercased name is not a hook by this convention, matching
/// eslint-plugin-react-refresh's `^use[A-Z0-9]` test.
fn is_react_hook_name(name: &str) -> bool {
    name.strip_prefix("use")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// True when `init` is function-shaped: an arrow function or a `function`
/// expression (after peeling `as const`, `satisfies`, `as T`, `<T>x`, `!`, and
/// parentheses). A custom hook is, by definition, such a value; an export named
/// like a hook but bound to a non-function value (`export const useStuff = {...}`)
/// is not a hook and is left subject to the rule.
fn is_function_expression(init: &Expression) -> bool {
    match init {
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => true,
        Expression::TSAsExpression(e) => is_function_expression(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_function_expression(&e.expression),
        Expression::TSTypeAssertion(e) => is_function_expression(&e.expression),
        Expression::TSNonNullExpression(e) => is_function_expression(&e.expression),
        Expression::ParenthesizedExpression(p) => is_function_expression(&p.expression),
        _ => false,
    }
}

/// True when `init` is a pure constant-data value: a literal primitive, a plain
/// object/array literal, or a template/unary/binary expression over those (after
/// peeling `as const`, `satisfies`, `as T`, `!`, and parentheses).
///
/// React Fast Refresh is only disrupted by exports that *could* be a component
/// or hook — i.e. function- or class-shaped values the HMR runtime might treat
/// as a component boundary. A string, number, boolean, plain object, or array is
/// inert data: it cannot be a component, so co-locating it with a component
/// export breaks nothing. Anything else (arrow/function/class expressions, calls
/// that may return a component or HOC) is left subject to the rule.
fn is_constant_data_initializer(init: &Expression) -> bool {
    match init {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_) => true,
        // `-1`, `!flag`, `24 * 60`, `'a' + 'b'` — constant when their operands are.
        Expression::UnaryExpression(u) => is_constant_data_initializer(&u.argument),
        Expression::BinaryExpression(b) => {
            is_constant_data_initializer(&b.left) && is_constant_data_initializer(&b.right)
        }
        // Wrappers that don't change the runtime value: `x as const`, `x satisfies
        // T`, `x as T`, `<T>x`, `x!`, `(x)`.
        Expression::TSAsExpression(e) => is_constant_data_initializer(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_constant_data_initializer(&e.expression),
        Expression::TSTypeAssertion(e) => is_constant_data_initializer(&e.expression),
        Expression::TSNonNullExpression(e) => is_constant_data_initializer(&e.expression),
        Expression::ParenthesizedExpression(p) => is_constant_data_initializer(&p.expression),
        _ => false,
    }
}

/// Next.js Pages Router data-fetching exports. The framework requires these to
/// live alongside the default page component in a page file — they cannot be
/// moved to a separate module, so flagging them as Fast-Refresh-breaking
/// non-component exports is a false positive.
const NEXT_PAGES_ROUTER_EXPORTS: &[&str] =
    &["getStaticProps", "getStaticPaths", "getServerSideProps"];

/// A file is a Next.js Pages Router page when it lives under a `pages/`
/// directory or imports from the `next` package (where `GetStaticProps`,
/// `GetServerSideProps`, etc. are declared).
fn is_next_pages_router_file(ctx: &CheckCtx) -> bool {
    ctx.file.path_segments.in_pages_router
        || crate::oxc_helpers::source_contains(ctx.source, "from 'next'")
        || crate::oxc_helpers::source_contains(ctx.source, "from \"next\"")
}

/// React Router v7 route-module convention exports. A route file can export
/// these framework-recognized names alongside its default page component; the
/// framework consumes them at build/runtime and React Fast Refresh tooling
/// tolerates them in this convention, so flagging them as Fast-Refresh-breaking
/// non-component exports is a false positive. PascalCase convention names
/// (`ErrorBoundary`, `HydrateFallback`, `Layout`) are already treated as
/// component exports and never need listing here.
const REACT_ROUTER_ROUTE_EXPORTS: &[&str] = &[
    "meta",
    "loader",
    "action",
    "middleware",
    "links",
    "headers",
    "clientLoader",
    "clientAction",
    "handle",
    "shouldRevalidate",
];

/// A file is a React Router v7 route module when it imports the framework's
/// auto-generated per-route types (`./+types/...`, emitted by `react-router
/// typegen`) or imports from the `react-router` package itself. Either import is
/// the framework's own convention marker for a route module, where `loader`,
/// `action`, `meta`, etc. are mandatory framework exports rather than incidental
/// non-component exports.
fn is_react_router_route_file(ctx: &CheckCtx) -> bool {
    crate::oxc_helpers::source_contains(ctx.source, "/+types/")
        || crate::oxc_helpers::source_contains(ctx.source, "from 'react-router'")
        || crate::oxc_helpers::source_contains(ctx.source, "from \"react-router\"")
}

/// Framework magic exports the file-system router consumes by convention, when
/// `path` is a Next.js App/Pages Router file. `metadata`, `generateMetadata`,
/// `revalidate`, `dynamic`, `generateStaticParams`, etc. must live next to the
/// default page/layout component — the framework reads them from the route
/// module itself, so flagging them as Fast-Refresh-breaking non-component
/// exports is a false positive. The names come from the central per-framework
/// magic-export registry (`magic_exports_for_path`), which only resolves them
/// when the file's package actually depends on the framework, so a coincidental
/// non-router export named `dynamic` elsewhere is still flagged.
fn next_router_magic_exports<'a>(ctx: &CheckCtx<'a>) -> std::collections::HashSet<&'a str> {
    if !(ctx.file.path_segments.in_app_router || ctx.file.path_segments.in_pages_router) {
        return std::collections::HashSet::new();
    }
    ctx.project.magic_exports_for_path(ctx.path)
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // TanStack Start uses Vite HMR — React Fast Refresh constraints don't apply.
        if ctx.project.framework == Framework::TanStackStart {
            return Vec::new();
        }

        // Publishable library source: React Fast Refresh is an app-bundler
        // concern. A file under a package that declares `main`/`module`/`exports`
        // is compiled into a distributable bundle, never loaded by the HMR
        // runtime, so co-locating utilities with components breaks nothing.
        if ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.is_library)
        {
            return Vec::new();
        }

        // Only check .tsx/.jsx files.
        let path_str = ctx.path.to_string_lossy();
        if !path_str.ends_with(".tsx") && !path_str.ends_with(".jsx") {
            return Vec::new();
        }

        // React Fast Refresh constraints only apply to files that actually use
        // React. A .tsx/.jsx file that pulls in no `react`/`react-dom` import
        // (e.g. a custom reactive UI framework with its own HMR, SolidJS, Preact,
        // Vue JSX) is not subject to Fast Refresh, so mixing component and
        // non-component exports breaks nothing.
        if !crate::oxc_helpers::imports_react(ctx.source) {
            return Vec::new();
        }

        let program = semantic.nodes().program();
        let next_pages_router = is_next_pages_router_file(ctx);
        let react_router_route = is_react_router_route_file(ctx);
        let next_magic_exports = next_router_magic_exports(ctx);

        let mut component_exports: Vec<String> = Vec::new();
        let mut non_component_exports: Vec<(String, usize)> = Vec::new();

        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(named) => {
                    if let Some(name) = extract_named_export_name(named, ctx.source) {
                        if next_pages_router
                            && NEXT_PAGES_ROUTER_EXPORTS.contains(&name.as_str())
                        {
                            continue;
                        }
                        if react_router_route
                            && REACT_ROUTER_ROUTE_EXPORTS.contains(&name.as_str())
                        {
                            continue;
                        }
                        if next_magic_exports.contains(name.as_str()) {
                            continue;
                        }
                        let offset = named.span.start as usize;
                        let (line, _) = byte_offset_to_line_col(ctx.source, offset);
                        if is_pascal_case(&name) {
                            component_exports.push(name);
                        } else {
                            non_component_exports.push((name, line));
                        }
                    }
                }
                Statement::ExportDefaultDeclaration(default_decl) => {
                    if let Some(name) = extract_default_export_name(&default_decl.declaration) {
                        let offset = default_decl.span.start as usize;
                        let (line, _) = byte_offset_to_line_col(ctx.source, offset);
                        if is_pascal_case(&name) {
                            component_exports.push(name);
                        } else {
                            non_component_exports.push((name, line));
                        }
                    }
                }
                _ => {}
            }
        }

        if component_exports.is_empty() || non_component_exports.is_empty() {
            return Vec::new();
        }

        non_component_exports
            .iter()
            .map(|(name, line)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: *line,
                column: 1,
                rule_id: "react-refresh-only-export-components".into(),
                message: format!(
                    "Non-component export `{name}` alongside component exports breaks React Fast Refresh. Move it to a separate module."
                ),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

fn extract_named_export_name(decl: &ExportNamedDeclaration, source: &str) -> Option<String> {
    // Skip re-exports (`export { ... } from '...'`).
    if decl.source.is_some() {
        return None;
    }
    // Skip `export type` / `export interface`.
    let text = source.get(decl.span.start as usize..decl.span.end as usize)?;
    if text.starts_with("export type ") || text.starts_with("export interface ") {
        return None;
    }
    let declaration = decl.declaration.as_ref()?;
    match declaration {
        Declaration::FunctionDeclaration(func) => {
            let name = func.id.as_ref()?.name.to_string();
            // A custom hook (`export function useFoo()`) is fast-refresh-safe to
            // co-export with a component — exempt it, mirroring
            // eslint-plugin-react-refresh.
            if is_react_hook_name(&name) {
                return None;
            }
            Some(name)
        }
        Declaration::ClassDeclaration(class) => {
            class.id.as_ref().map(|id| id.name.to_string())
        }
        Declaration::VariableDeclaration(var_decl) => {
            var_decl.declarations.first().and_then(|d| {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &d.id {
                    // A constant-data export (string/number/object/array literal,
                    // etc.) cannot be a component or hook and so cannot disrupt
                    // React Fast Refresh — exempt it from the rule.
                    if d.init.as_ref().is_some_and(is_constant_data_initializer) {
                        return None;
                    }
                    // A custom hook bound to a function (`export const useFoo =
                    // () => ...`) is fast-refresh-safe to co-export with a
                    // component — exempt it. The function shape is required: an
                    // export named like a hook but holding data (`export const
                    // useStuff = {...}`) is not a hook and stays subject to the
                    // rule.
                    if is_react_hook_name(&ident.name)
                        && d.init.as_ref().is_some_and(is_function_expression)
                    {
                        return None;
                    }
                    Some(ident.name.to_string())
                } else {
                    None
                }
            })
        }
        _ => None,
    }
}

fn extract_default_export_name(decl: &ExportDefaultDeclarationKind) -> Option<String> {
    match decl {
        ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
            func.id.as_ref().map(|id| id.name.to_string())
        }
        ExportDefaultDeclarationKind::ClassDeclaration(class) => {
            class.id.as_ref().map(|id| id.name.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Framework, ProjectCtx};

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    fn run_tanstack(source: &str) -> Vec<Diagnostic> {
        let mut project = ProjectCtx::default();
        project.framework = Framework::TanStackStart;
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_mixed_exports() {
        let source = r#"
import React from 'react';
export function MyComponent() { return <div />; }
export const helper = () => {};
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn allows_component_only_exports() {
        let source = r#"
export function MyComponent() { return <div />; }
export function AnotherComponent() { return <span />; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_type_exports_with_components() {
        let source = r#"
export type Props = { name: string };
export interface Config { debug: boolean }
export function MyComponent() { return <div />; }
"#;
        assert!(run(source).is_empty());
    }

    // Regression tests for issue #457 — TanStack Start / Vite HMR co-location patterns.

    #[test]
    fn no_fp_tanstack_start_utility_function_with_component() {
        // getProductsColumns co-located with DataTableHeader (issue #457)
        let source = r#"
import React from 'react';
export function getProductsColumns(t) { return []; }
export function DataTableHeader() { return <div />; }
"#;
        assert!(run_tanstack(source).is_empty());
    }

    #[test]
    fn no_fp_tanstack_start_cva_variant_with_component() {
        // badgeVariants CVA co-located with Badge (issue #457)
        let source = r#"
import React from 'react';
export const badgeVariants = cva("badge", { variants: {} });
export function Badge({ className }) { return <div className={className} />; }
"#;
        assert!(run_tanstack(source).is_empty());
    }

    #[test]
    fn no_fp_tanstack_start_context_hook_with_provider() {
        // useSidebar hook co-located with SidebarProvider (issue #457)
        let source = r#"
import React from 'react';
export function useSidebar() { return useContext(SidebarContext); }
export function SidebarProvider({ children }) { return <div>{children}</div>; }
"#;
        assert!(run_tanstack(source).is_empty());
    }

    // Regression tests for issue #1896 — Next.js Pages Router data-fetching
    // exports (getStaticProps/getStaticPaths/getServerSideProps) are framework
    // conventions that must live alongside the page component.

    #[test]
    fn no_fp_next_pages_router_get_static_exports() {
        // website/src/pages/docs/[...slug].tsx (issue #1896)
        let source = r#"
import React from 'react';
import { GetStaticPaths, GetStaticProps } from 'next';

export default function DocsPage() { return <div />; }

export const getStaticProps: GetStaticProps = async (context) => { return { props: {} }; };

export const getStaticPaths: GetStaticPaths = async () => { return { paths: [], fallback: false }; };
"#;
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            source,
            "website/src/pages/docs/[...slug].tsx",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn no_fp_next_pages_router_get_server_side_props() {
        let source = r#"
import React from 'react';
export default function Page() { return <div />; }
export const getServerSideProps = async () => { return { props: {} }; };
"#;
        let d =
            crate::rules::test_helpers::run_rule_gated(&Check, source, "src/pages/profile.tsx");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_non_convention_export_in_pages_router() {
        // A genuine non-component export still breaks Fast Refresh, even in a
        // Pages Router file — the exemption is scoped to the convention names.
        let source = r#"
import React from 'react';
export default function Page() { return <div />; }
export const helper = () => {};
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "src/pages/index.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn flags_get_static_props_outside_next_context() {
        // Same name but neither under pages/ nor importing from next — not a
        // Pages Router page, so the export is still flagged.
        let source = r#"
import React from 'react';
export function MyComponent() { return <div />; }
export const getStaticProps = async () => {};
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "src/components/Foo.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getStaticProps"));
    }

    // Regression tests for issue #1801 — React Router v7 route modules export
    // framework-recognized convention names (meta/loader/action/middleware/...)
    // alongside the default page component. The framework consumes these at
    // build/runtime and Fast Refresh tooling tolerates them in this convention.

    #[test]
    fn no_fp_react_router_v7_meta_with_component() {
        // integration/templates/react-router-node/app/routes/home.tsx (issue #1801)
        let source = r#"
import React from 'react';
import { Show, UserButton } from '@clerk/react-router';
import type { Route } from './+types/home';

export function meta({}: Route.MetaArgs) {
  return [{ title: 'New React Router App' }, { name: 'description', content: 'Welcome to React Router!' }];
}

export default function Home() {
  return (
    <div>
      <UserButton />
    </div>
  );
}
"#;
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            source,
            "app/routes/home.tsx",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn no_fp_react_router_v7_middleware_and_loader_in_root() {
        // integration/templates/react-router-node/app/root.tsx (issue #1801)
        let source = r#"
import React from 'react';
import type { Route } from './+types/root';

export const middleware: Route.MiddlewareFunction[] = [clerkMiddleware()];
export const loader = (args: Route.LoaderArgs) => rootAuthLoader(args);

export function Layout({ children }: { children: React.ReactNode }) { return <html>{children}</html>; }
export default function App() { return <Outlet />; }
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "app/root.tsx");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_non_convention_export_in_react_router_route() {
        // A genuine non-component export still breaks Fast Refresh, even in a
        // React Router route file — the exemption is scoped to the convention names.
        let source = r#"
import React from 'react';
import type { Route } from './+types/home';

export const helper = () => {};
export default function Home() { return <div />; }
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "app/routes/home.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn flags_loader_outside_react_router_context() {
        // Same convention name but the file is neither a `+types/` route module
        // nor importing from `react-router` — still flagged.
        let source = r#"
import React from 'react';
export function MyComponent() { return <div />; }
export const loader = async () => {};
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "src/components/Foo.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("loader"));
    }

    // Regression tests for issue #1898 — publishable npm library source files.
    // React Fast Refresh is an app-bundler concern; a file under a package whose
    // package.json declares `main`/`module`/`exports` is bundled and never
    // loaded by the HMR runtime, so co-locating utilities with components is not
    // a false positive.

    /// Run the check against `source` with a real `ProjectCtx` rooted at a
    /// tempdir whose `package.json` is `pkg_json` — exercises the library-source
    /// relaxation, which depends on `nearest_package_json`.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join("src/FieldArray.tsx");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::Tsx,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();

        let allocator = Allocator::default();
        let source_type = crate::oxc_helpers::source_type_for_path(&canon);
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic =
            oxc_semantic::SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn no_fp_library_source_co_located_utilities() {
        // packages/formik/src/FieldArray.tsx (issue #1898): utility helpers
        // co-located with a component in a publishable package's source.
        let source = r#"
import React from 'react';
export const move = (array, from, to) => { return array; };
export const swap = (array, a, b) => { return array; };
export const FieldArray = connect(FieldArrayInner);
"#;
        let d = run_with_pkg(r#"{ "name": "formik", "main": "dist/formik.cjs.js" }"#, source);
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_app_source_co_located_utilities() {
        // Same code under a private app package (no main/module/exports) still
        // breaks Fast Refresh — the exemption is scoped to publishable libraries.
        let source = r#"
import React from 'react';
export const move = (array, from, to) => { return array; };
export const FieldArray = connect(FieldArrayInner);
"#;
        let d = run_with_pkg(r#"{ "name": "my-app", "private": true }"#, source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("move"));
    }

    // Regression tests for issue #1647 — a .tsx file that uses JSX via a custom,
    // non-React UI framework (no `react`/`react-dom` import) is not subject to
    // React Fast Refresh, so mixing component and non-component exports is not a
    // false positive.

    #[test]
    fn no_fp_non_react_custom_ui_framework() {
        // packages/ui/src/components/tabs/tabs.tsx (issue #1647): a custom
        // reactive framework with its own HMR, no `react` import.
        let source = r#"
import {
  attrs, css, createElement, createMixin, on,
  type Handle, type MixinHandle, type Props,
} from '@remix-run/ui';

export const listStyle = tabsListCss;
export const triggerStyle = tabsTriggerCss;

export const list = listMixin;
export const panel = panelMixin;
export const trigger = triggerMixin;

export function Tabs(handle) {
  return () => createElement('div');
}
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn flags_same_shape_with_react_import() {
        // The identical mixed-export shape *with* a `react` import is genuinely
        // subject to Fast Refresh and is still flagged — the gate is scoped to
        // files that do not use React.
        let source = r#"
import * as React from 'react';

export const listStyle = tabsListCss;
export function Tabs() { return <div />; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("listStyle"));
    }

    // Regression tests for issue #1866 — test-directory helper .tsx files. React
    // Fast Refresh is a dev-server HMR concern; test files are loaded by the test
    // runner (Vitest/Jest), never the HMR runtime, so mixing component test cases
    // with utility helpers is not a false positive.

    #[test]
    fn no_fp_test_helper_mixed_exports() {
        // test/helper/parameterizedTestCases.tsx (issue #1866): component test
        // cases co-located with an `includingCompact` utility function.
        let source = r#"
import React from 'react';
export const BarChartCase = { name: 'BarChart', Comp: BarChart };
export function includingCompact(testCases) { return testCases; }
export const allCartesianChartCases = [BarChartCase];
"#;
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            source,
            "test/helper/parameterizedTestCases.tsx",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn no_fp_test_vr_directory_mixed_exports() {
        // Visual-regression test directory (recharts test-vr/) — also never
        // loaded under Fast Refresh.
        let source = r#"
import React from 'react';
export function Chart() { return <div />; }
export const helper = () => {};
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "test-vr/charts.tsx");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_production_source_mixed_exports() {
        // Same mixed-export shape in production source still breaks Fast Refresh —
        // the exemption is scoped to test directories.
        let source = r#"
import React from 'react';
export function Chart() { return <div />; }
export const helper = () => {};
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "src/Foo.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    // Regression tests for issue #1630 — Next.js App Router pages/layouts export
    // framework-consumed directives (metadata/revalidate/dynamic/...) alongside
    // the default page component. Next.js reads these from the route module by
    // convention and its own HMR pipeline tolerates them, so flagging them as
    // Fast-Refresh-breaking non-component exports is a false positive.

    /// Run the check with a real `ProjectCtx` rooted at a tempdir whose
    /// `package.json` is `pkg_json`, with `source` written to `rel_path`.
    /// Exercises the magic-export exemption, which depends on framework
    /// detection via `magic_exports_for_path`.
    fn run_at_path(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile { path: file_path.clone(), language: Language::Tsx };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();
        let lang = Language::Tsx;
        let file = crate::rules::file_ctx::FileCtx::build(&canon, source, lang, &project);

        let allocator = Allocator::default();
        let source_type = crate::oxc_helpers::source_type_for_path(&canon);
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic =
            oxc_semantic::SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_full(&canon, source, &project, &file);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn no_fp_next_app_router_metadata_with_default_component() {
        // apps/v4/app/(app)/blocks/layout.tsx (issue #1630): App Router layout
        // exporting `metadata` alongside the default component.
        let source = r#"
import { type Metadata } from "next"
import React from "react"

export const metadata: Metadata = {
  title: "Building Blocks for the Web",
  description: "Clean, modern building blocks.",
}

export default function BlocksLayout({ children }: { children: React.ReactNode }) {
  return <div>{children}</div>
}
"#;
        let d = run_at_path(
            r#"{ "name": "v4", "dependencies": { "next": "15.0.0", "react": "19.0.0" } }"#,
            "app/(app)/blocks/layout.tsx",
            source,
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn no_fp_next_app_router_revalidate_and_dynamic() {
        // App Router page exporting route-segment config directives.
        let source = r#"
import React from "react"

export const revalidate = 60
export const dynamic = "force-dynamic"

export default function Page() { return <div /> }
"#;
        let d = run_at_path(
            r#"{ "name": "site", "dependencies": { "next": "15.0.0", "react": "19.0.0" } }"#,
            "app/page.tsx",
            source,
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_non_magic_export_in_next_app_router() {
        // A genuine non-component export (not a Next.js magic name) alongside a
        // component still breaks Fast Refresh, even in a Next.js App Router file —
        // the exemption is scoped to framework-recognized magic exports.
        let source = r#"
import React from "react"

export const helper = () => {}

export default function Page() { return <div /> }
"#;
        let d = run_at_path(
            r#"{ "name": "site", "dependencies": { "next": "15.0.0", "react": "19.0.0" } }"#,
            "app/page.tsx",
            source,
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    // Regression tests for issue #2229 — plain module-level constant exports
    // (string/number/boolean literals, plain objects, arrays, `as const`) are
    // inert data: they cannot be a component or hook, so co-locating them with a
    // component export breaks nothing under React Fast Refresh. Only
    // function/component/hook-shaped exports are relevant to Fast Refresh.

    #[test]
    fn no_fp_constant_string_exports_with_component() {
        // app/routes/_auth/verify.tsx (issue #2229): route-specific query-param
        // name constants co-located with the form component.
        let source = r#"
import React from 'react';
export const codeQueryParam = 'code';
export const targetQueryParam = 'target';
export function VerifyForm() { return <div />; }
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn no_fp_constant_object_and_array_exports_with_component() {
        // app/utils/connections.tsx (issue #2229): lookup maps and `as const`
        // arrays co-located with a component.
        let source = r#"
import React from 'react';
export const providerLabels = { github: 'GitHub', google: 'Google' };
export const providerNames = ['github'] as const;
export function ProviderConnectionForm() { return <div />; }
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn no_fp_constant_default_string_export_with_component() {
        // app/routes/_auth/onboarding/index.tsx (issue #2229): a session-key
        // string constant co-located with the default page component.
        let source = r#"
import React from 'react';
export const onboardingEmailSessionKey = 'onboardingEmail';
export default function Onboarding() { return <div />; }
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn no_fp_numeric_and_boolean_constant_exports_with_component() {
        let source = r#"
import React from 'react';
export const maxItems = 24 * 60;
export const isEnabled = true;
export const label = `prefix-${SOME}`;
export function Widget() { return <div />; }
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn flags_arrow_function_export_with_component() {
        // Negative-space guard: a non-hook function-shaped const export (a plain
        // utility, not a `use*` hook) is the real Fast Refresh risk and is still
        // flagged.
        let source = r#"
import React from 'react';
export const doThing = () => { return 1; };
export function Widget() { return <div />; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("doThing"));
    }

    #[test]
    fn flags_call_expression_export_with_component() {
        // Negative-space guard: a call may return a component or HOC, so it stays
        // subject to the rule — only inert data values are exempt.
        let source = r#"
import React from 'react';
export const helper = makeHelper();
export function Widget() { return <div />; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    // Regression tests for issue #2122 — a custom React hook (`use*` naming
    // convention) bound to a function is fast-refresh-safe to co-export alongside
    // a component (the idiomatic `QueryClientProvider` + `useQueryClient`
    // pattern). eslint-plugin-react-refresh does not warn on hook exports.

    #[test]
    fn no_fp_arrow_hook_co_exported_with_provider_component() {
        // packages/react-query/src/QueryClientProvider.tsx (issue #2122).
        let source = r#"
import React from 'react';
export const useQueryClient = (queryClient) => {
  return React.useContext(QueryClientContext);
};
export const QueryClientProvider = ({ children, client }) => {
  return <QueryClientContext.Provider value={client}>{children}</QueryClientContext.Provider>;
};
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn no_fp_function_declaration_hook_co_exported_with_component() {
        let source = r#"
import React from 'react';
export function useQueryClient() { return React.useContext(Ctx); }
export function QueryClientProvider(props) {
  return <Ctx.Provider>{props.children}</Ctx.Provider>;
}
"#;
        assert!(run(source).is_empty(), "expected no diagnostics, got {:?}", run(source));
    }

    #[test]
    fn flags_genuine_non_component_export_alongside_hook_and_component() {
        // Negative-space guard: a genuinely disallowed non-component export (a
        // plain non-hook function) is still flagged even when a component and a
        // hook are also present — the exemption is scoped to `use*` functions.
        let source = r#"
import React from 'react';
export const useQueryClient = () => React.useContext(Ctx);
export const helper = () => {};
export function QueryClientProvider(props) { return <Ctx.Provider />; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("helper"));
    }

    #[test]
    fn flags_hook_named_non_function_export_with_component() {
        // Negative-space guard: an export named like a hook but bound to a
        // non-function, non-data value (a call, which may return anything) is not
        // a hook and does not get the hook exemption — it stays subject to the
        // rule.
        let source = r#"
import React from 'react';
export const useStuff = makeStuff();
export function Widget() { return <div />; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("useStuff"));
    }
}
