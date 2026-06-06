use std::path::Path;

use crate::project::{ModuleType, ProjectCtx};
use crate::rules::backend::CheckCtx;

/// `is_es_module_context` for per-node `OxcCheck::run` hot paths. The result is
/// file-invariant but `nearest_package_json` walks/locks per call, so memoize
/// it for the current file (cleared once per file by `reset_file_caches`).
#[must_use]
pub(crate) fn is_es_module_context_cached(ctx: &CheckCtx) -> bool {
    crate::oxc_helpers::cached_file_bool(ctx.source, crate::oxc_helpers::SLOT_ES_MODULE, || {
        is_es_module_context(ctx.path, ctx.project)
    })
}

#[must_use]
pub(crate) fn is_es_module_context(path: &Path, project: &ProjectCtx) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("mjs") => return true,
        Some(extension) if extension.eq_ignore_ascii_case("mts") => return true,
        Some(extension) if extension.eq_ignore_ascii_case("cjs") => return false,
        Some(extension) if extension.eq_ignore_ascii_case("cts") => return false,
        _ => {}
    }

    project
        .nearest_package_json(path)
        .is_some_and(|package| package.module_type == ModuleType::Module)
}
