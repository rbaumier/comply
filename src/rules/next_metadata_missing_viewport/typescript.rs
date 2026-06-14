//! next-metadata-missing-viewport backend.
//!
//! Triggers once per `layout` file when a `metadata` export is present but no
//! `viewport` (or `generateViewport`) export accompanies it. Next 14+ split
//! viewport/themeColor out of the metadata API. `viewport` is hierarchical:
//! a `layout` defines it and nested pages inherit it, so only `layout` files
//! are checked.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

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

crate::ast_check! { on ["program"] => |node, _source, ctx, diagnostics|
    if ctx.project.framework != Framework::NextJs {
        return;
    }
    if !is_layout_file(ctx.path) {
        return;
    }
    if !source_has_metadata_export(ctx.source) {
        return;
    }
    if source_has_viewport_export(ctx.source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "next-metadata-missing-viewport".into(),
        message: "Layout exports `metadata` but not `viewport` — add a `viewport` export with `width: 'device-width'` so nested pages inherit it.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run_at(source: &str, path: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &FileCtx::default())
    }

    #[test]
    fn flags_layout_metadata_without_viewport() {
        let src = "export const metadata = { title: 'X' };\nexport default function Layout({ children }) { return <div>{children}</div>; }";
        assert_eq!(run_at(src, "app/layout.tsx", &next_project()).len(), 1);
    }

    #[test]
    fn allows_layout_metadata_with_viewport() {
        let src = "export const metadata = { title: 'X' };\nexport const viewport = { width: 'device-width' };\nexport default function Layout({ children }) { return <div>{children}</div>; }";
        assert!(run_at(src, "app/layout.tsx", &next_project()).is_empty());
    }

    #[test]
    fn allows_no_metadata() {
        let src = "export default function Layout({ children }) { return <div>{children}</div>; }";
        assert!(run_at(src, "app/layout.tsx", &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export const metadata = { title: 'X' };";
        assert!(run_at(src, "app/layout.tsx", &ProjectCtx::empty()).is_empty());
    }

    // Regression #2219: child pages inherit `viewport` from the nearest parent
    // layout, so a `page` file exporting metadata without its own viewport is
    // never a defect.
    #[test]
    fn no_fp_page_metadata_without_viewport() {
        let src = "export async function generateMetadata(props) { return { title: 'X' }; }\nexport default async function Page(props) { return <div />; }";
        assert!(run_at(src, "app/product/[handle]/page.tsx", &next_project()).is_empty());
    }
}
