//! react-refresh-only-export-components oxc backend for TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Declaration, ExportDefaultDeclarationKind, ExportNamedDeclaration, Statement,
};
use std::sync::Arc;

pub struct Check;

fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
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

        let program = semantic.nodes().program();
        let next_pages_router = is_next_pages_router_file(ctx);

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
            func.id.as_ref().map(|id| id.name.to_string())
        }
        Declaration::ClassDeclaration(class) => {
            class.id.as_ref().map(|id| id.name.to_string())
        }
        Declaration::VariableDeclaration(var_decl) => {
            var_decl.declarations.first().and_then(|d| {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &d.id {
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
export function getProductsColumns(t) { return []; }
export function DataTableHeader() { return <div />; }
"#;
        assert!(run_tanstack(source).is_empty());
    }

    #[test]
    fn no_fp_tanstack_start_cva_variant_with_component() {
        // badgeVariants CVA co-located with Badge (issue #457)
        let source = r#"
export const badgeVariants = cva("badge", { variants: {} });
export function Badge({ className }) { return <div className={className} />; }
"#;
        assert!(run_tanstack(source).is_empty());
    }

    #[test]
    fn no_fp_tanstack_start_context_hook_with_provider() {
        // useSidebar hook co-located with SidebarProvider (issue #457)
        let source = r#"
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
export function MyComponent() { return <div />; }
export const getStaticProps = async () => {};
"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, source, "src/components/Foo.tsx");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getStaticProps"));
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
export const move = (array, from, to) => { return array; };
export const FieldArray = connect(FieldArrayInner);
"#;
        let d = run_with_pkg(r#"{ "name": "my-app", "private": true }"#, source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("move"));
    }
}
