//! Shared Playwright rule scoping.

use crate::rules::backend::CheckCtx;

#[must_use]
pub fn imports_playwright_test(source: &str) -> bool {
    source.contains("from \"@playwright/test\"")
        || source.contains("from '@playwright/test'")
        || source.contains("require(\"@playwright/test\")")
        || source.contains("require('@playwright/test')")
        || source.contains("import(\"@playwright/test\")")
        || source.contains("import('@playwright/test')")
}

#[must_use]
pub fn is_playwright_context(ctx: &CheckCtx) -> bool {
    if imports_playwright_test(ctx.source) {
        return true;
    }
    if !ctx.project.has_framework("playwright") {
        return false;
    }
    let path = ctx.path.to_string_lossy();
    path.contains("/e2e/") || path.contains("/playwright/") || path.contains(".e2e.")
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
