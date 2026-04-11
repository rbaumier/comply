//! playwright-prefer-native-locators — flag `locator('[role="..."]')` in favor of `getByRole()` etc.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-native-locators",
    description: "`locator('[role=\"button\"]')` should be `getByRole('button')` — use Playwright's built-in locators.",
    remediation: "Replace attribute-selector locators with Playwright's \
                  built-in locator methods: `[role=...]` → `getByRole()`, \
                  `[placeholder=...]` → `getByPlaceholder()`, \
                  `[alt=...]` → `getByAltText()`, \
                  `[title=...]` → `getByTitle()`, \
                  `[data-testid=...]` → `getByTestId()`. \
                  Built-in locators are more readable and provide better \
                  error messages.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
