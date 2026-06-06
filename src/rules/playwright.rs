//! Shared Playwright rule scoping.

use crate::rules::backend::CheckCtx;

#[must_use]
pub fn imports_playwright_test(source: &str) -> bool {
    use crate::oxc_helpers::source_contains;
    source_contains(source, "from \"@playwright/test\"")
        || source_contains(source, "from '@playwright/test'")
        || source_contains(source, "require(\"@playwright/test\")")
        || source_contains(source, "require('@playwright/test')")
        || source_contains(source, "import(\"@playwright/test\")")
        || source_contains(source, "import('@playwright/test')")
}

#[must_use]
pub fn is_playwright_context(ctx: &CheckCtx) -> bool {
    // File-invariant (source + path + project), but called from per-node
    // `run()` across ~25 Playwright rules — without memoization each
    // CallExpression repays `to_string_lossy` + the import/path scans. Cache
    // the answer once per file.
    crate::oxc_helpers::cached_file_bool(ctx.source, crate::oxc_helpers::SLOT_PLAYWRIGHT, || {
        if imports_playwright_test(ctx.source) {
            return true;
        }
        if !ctx.project.has_framework("playwright") {
            return false;
        }
        let path = ctx.path.to_string_lossy();
        path.contains("/e2e/") || path.contains("/playwright/") || path.contains(".e2e.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_static_import() {
        assert!(imports_playwright_test(
            "import { test } from \"@playwright/test\";"
        ));
    }

    #[test]
    fn ignores_string_marker_without_import() {
        assert!(!imports_playwright_test(
            "const packageName = \"@playwright/test\";"
        ));
    }
}
