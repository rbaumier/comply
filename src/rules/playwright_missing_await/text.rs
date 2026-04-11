use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Known Playwright async methods that must be awaited.
const ASYNC_METHODS: &[&str] = &[
    // Page methods.
    "page.goto",
    "page.click",
    "page.dblclick",
    "page.fill",
    "page.type",
    "page.press",
    "page.check",
    "page.uncheck",
    "page.selectOption",
    "page.setInputFiles",
    "page.waitForSelector",
    "page.waitForNavigation",
    "page.waitForLoadState",
    "page.waitForURL",
    "page.waitForEvent",
    "page.waitForFunction",
    "page.waitForTimeout",
    "page.waitForResponse",
    "page.waitForRequest",
    "page.screenshot",
    "page.pdf",
    "page.content",
    "page.title",
    "page.evaluate",
    "page.evaluateHandle",
    "page.reload",
    "page.goBack",
    "page.goForward",
    "page.close",
    "page.hover",
    "page.focus",
    "page.tap",
    "page.dragAndDrop",
    "page.setContent",
    "page.addInitScript",
    "page.route",
    "page.unroute",
    "page.exposeFunction",
    "page.emulateMedia",
    "page.setViewportSize",
    "page.setExtraHTTPHeaders",
    // Locator methods.
    "locator.click",
    "locator.dblclick",
    "locator.fill",
    "locator.type",
    "locator.press",
    "locator.check",
    "locator.uncheck",
    "locator.selectOption",
    "locator.setInputFiles",
    "locator.hover",
    "locator.focus",
    "locator.tap",
    "locator.dragTo",
    "locator.screenshot",
    "locator.scrollIntoViewIfNeeded",
    "locator.selectText",
    "locator.setChecked",
    "locator.inputValue",
    "locator.textContent",
    "locator.innerText",
    "locator.innerHTML",
    "locator.getAttribute",
    "locator.isVisible",
    "locator.isHidden",
    "locator.isEnabled",
    "locator.isDisabled",
    "locator.isChecked",
    "locator.isEditable",
    "locator.boundingBox",
    "locator.evaluate",
    "locator.evaluateHandle",
    "locator.waitFor",
    "locator.clear",
    // Browser / BrowserContext.
    "browser.newPage",
    "browser.newContext",
    "browser.close",
    "context.newPage",
    "context.close",
    "context.route",
    "context.clearCookies",
    "context.addCookies",
    "context.cookies",
    "context.storageState",
    // Frame.
    "frame.click",
    "frame.fill",
    "frame.goto",
    "frame.waitForSelector",
    "frame.evaluate",
];

/// Known assertion methods (expect().toBeX) that are async in Playwright.
const ASYNC_EXPECT_METHODS: &[&str] = &[
    "toBeVisible",
    "toBeHidden",
    "toBeEnabled",
    "toBeDisabled",
    "toBeChecked",
    "toBeEditable",
    "toBeEmpty",
    "toBeFocused",
    "toBeAttached",
    "toBeInViewport",
    "toContainText",
    "toHaveAttribute",
    "toHaveClass",
    "toHaveCount",
    "toHaveCSS",
    "toHaveId",
    "toHaveJSProperty",
    "toHaveScreenshot",
    "toHaveText",
    "toHaveTitle",
    "toHaveURL",
    "toHaveValue",
    "toHaveValues",
    "toPass",
];

fn check_line(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Skip comments.
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return None;
    }

    // Check for Playwright page/locator/browser/context/frame methods.
    for method in ASYNC_METHODS {
        let parts: Vec<&str> = method.split('.').collect();
        if parts.len() != 2 {
            continue;
        }
        let method_call = parts[1];
        let object_suffix = format!(".{method_call}(");

        if let Some(call_pos) = trimmed.find(&object_suffix)
            && call_pos > 0
        {
            let before = &trimmed[..call_pos];
            if before.contains("await ") || before.ends_with("await") {
                continue;
            }
            let last_token = before
                .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == ';')
                .next()
                .unwrap_or("");
            let valid_objects = ["page", "locator", "browser", "context", "frame"];
            let is_playwright_obj = valid_objects.iter().any(|obj| {
                last_token == *obj
                    || last_token.ends_with(&format!(".{obj}"))
                    || last_token.ends_with(&format!("_{obj}"))
            });
            if is_playwright_obj {
                return Some(format!("{last_token}.{method_call}"));
            }
        }
    }

    // Check for expect(...).toBeX / expect(...).toHaveX patterns.
    for method in ASYNC_EXPECT_METHODS {
        let pattern = format!(".{method}(");
        if trimmed.contains(&pattern)
            && trimmed.contains("expect(")
            && !trimmed.contains("await ")
            && !trimmed.starts_with("await")
        {
            return Some(format!("expect(...).{method}"));
        }
    }

    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(call) = check_line(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "playwright-missing-await".into(),
                    message: format!(
                        "`{call}` is an async Playwright method — add `await` to prevent flaky tests."
                    ),
                    severity: Severity::Error,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("test.spec.ts"), source))
    }

    #[test]
    fn flags_missing_await_on_page_click() {
        let source = "page.click('#button');";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("page.click"));
    }

    #[test]
    fn flags_missing_await_on_expect() {
        let source = "expect(locator).toBeVisible();";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeVisible"));
    }

    #[test]
    fn allows_awaited_calls() {
        let source = r#"
await page.click('#button');
await expect(locator).toBeVisible();
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_locator_fill() {
        let source = "locator.fill('hello');";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fill"));
    }
}
