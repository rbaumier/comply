//! playwright-prefer-native-locators — flag `locator('[role="..."]')` in favor of `getByRole()` etc.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
