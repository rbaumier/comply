use std::path::Path;

use crate::project::{ModuleType, ProjectCtx};

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
