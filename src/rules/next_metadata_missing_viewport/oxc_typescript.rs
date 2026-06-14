use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

/// `viewport` is hierarchical in the App Router: it is defined once in a
/// `layout` file and inherited by every nested page. A `page` file exporting
/// metadata without its own `viewport` is therefore never a defect — it
/// inherits viewport from the nearest parent layout. Only `layout` files are
/// the place a missing `viewport` matters, so scope the check to them.
fn is_layout_file(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem == "layout")
}

fn source_has_viewport_export(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "export const viewport")
        || crate::oxc_helpers::source_contains(source, "export function generateViewport")
        || crate::oxc_helpers::source_contains(source, "export async function generateViewport")
        || crate::oxc_helpers::source_contains(source, "export let viewport")
        || crate::oxc_helpers::source_contains(source, "export var viewport")
}

fn source_has_metadata_export(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "export const metadata")
        || crate::oxc_helpers::source_contains(source, "export function generateMetadata")
        || crate::oxc_helpers::source_contains(source, "export async function generateMetadata")
        || crate::oxc_helpers::source_contains(source, "export let metadata")
        || crate::oxc_helpers::source_contains(source, "export var metadata")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.project.framework != Framework::NextJs {
            return Vec::new();
        }
        if !is_layout_file(ctx.path) {
            return Vec::new();
        }
        if !source_has_metadata_export(ctx.source) {
            return Vec::new();
        }
        if source_has_viewport_export(ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Layout exports `metadata` but not `viewport` \u{2014} add a `viewport` export with `width: 'device-width'` so nested pages inherit it.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use crate::rules::file_ctx::FileCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::path::Path;

    fn next_project() -> crate::project::ProjectCtx {
        let mut project = crate::project::ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, path: &str, project: &crate::project::ProjectCtx) -> Vec<Diagnostic> {
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let file = FileCtx::default();
        let ctx = CheckCtx::for_test_full(Path::new(path), source, project, &file);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn flags_layout_metadata_without_viewport() {
        let src = "export const metadata = { title: 'X' };\nexport default function Layout({ children }) { return <div>{children}</div>; }";
        assert_eq!(run(src, "app/layout.tsx", &next_project()).len(), 1);
    }

    #[test]
    fn allows_layout_metadata_with_viewport() {
        let src = "export const metadata = { title: 'X' };\nexport const viewport = { width: 'device-width' };\nexport default function Layout({ children }) { return <div>{children}</div>; }";
        assert!(run(src, "app/layout.tsx", &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export const metadata = { title: 'X' };";
        assert!(run(src, "app/layout.tsx", &crate::project::ProjectCtx::empty()).is_empty());
    }

    // Regression #2219: child pages inherit `viewport` from the nearest parent
    // layout, so a `page` file exporting metadata without its own viewport is
    // never a defect (vercel/commerce: app/product/[handle]/page.tsx etc.).
    #[test]
    fn no_fp_page_metadata_without_viewport() {
        let src = "export async function generateMetadata(props) { return { title: 'X' }; }\nexport default async function Page(props) { return <div />; }";
        assert!(run(src, "app/product/[handle]/page.tsx", &next_project()).is_empty());
    }

    #[test]
    fn no_fp_root_page_metadata_without_viewport() {
        let src = "export const metadata = { title: 'Home' };\nexport default function Page() { return <div />; }";
        assert!(run(src, "app/page.tsx", &next_project()).is_empty());
    }
}
