//! next-metadata-missing-viewport backend.
//!
//! Triggers once per file when a `metadata` export is present but no
//! `viewport` (or `generateViewport`) export accompanies it. Next 14+ split
//! viewport/themeColor out of the metadata API.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

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
        message: "Page exports `metadata` but not `viewport` — add a `viewport` export with `width: 'device-width'`.".into(),
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

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", project, &FileCtx::default())
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
