use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

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
            message: "Page exports `metadata` but not `viewport` \u{2014} add a `viewport` export with `width: 'device-width'`.".into(),
            severity: Severity::Warning,
            span: None,
        }]
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


    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(
            source,
            &Check,
            project)
    }


    #[test]
    fn flags_metadata_without_viewport() {
        let src = "export const metadata = { title: 'X' };\nexport default function Page() { return <div />; }";
        assert_eq!(run(src, &next_project()).len(), 1);
    }


    #[test]
    fn allows_metadata_with_viewport() {
        let src = "export const metadata = { title: 'X' };\nexport const viewport = { width: 'device-width' };\nexport default function Page() { return <div />; }";
        assert!(run(src, &next_project()).is_empty());
    }


    #[test]
    fn allows_no_metadata() {
        let src = "export default function Page() { return <div />; }";
        assert!(run(src, &next_project()).is_empty());
    }


    #[test]
    fn ignores_non_nextjs_project() {
        let src = "export const metadata = { title: 'X' };";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
