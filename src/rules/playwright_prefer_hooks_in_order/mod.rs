//! playwright-prefer-hooks-in-order — enforce consistent hook ordering.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-hooks-in-order",
    description: "Hooks should follow the lifecycle order: beforeAll, beforeEach, afterEach, afterAll.",
    remediation: "Reorder hooks to: `beforeAll` > `beforeEach` > `afterEach` > `afterAll`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-hooks-in-order.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
