//! next-metadata-missing-viewport backend.
//!
//! Triggers once per file when a `metadata` export is present but no
//! `viewport` (or `generateViewport`) export accompanies it. Next 14+ split
//! viewport/themeColor out of the metadata API.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;

fn source_has_viewport_export(source: &str) -> bool {
    source.contains("export const viewport")
        || source.contains("export function generateViewport")
        || source.contains("export async function generateViewport")
        || source.contains("export let viewport")
        || source.contains("export var viewport")
}

fn source_has_metadata_export(source: &str) -> bool {
    source.contains("export const metadata")
        || source.contains("export function generateMetadata")
        || source.contains("export async function generateMetadata")
        || source.contains("export let metadata")
        || source.contains("export var metadata")
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
        crate::rules::test_helpers::run_tsx_with_project_and_file(
            source,
            &Check,
            project,
            &FileCtx::default(),
        )
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
