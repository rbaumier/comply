//! ts-no-restricted-imports — disallow imports whose module specifier
//! matches a user-configured pattern list.
//!
//! Opt-in via `comply.toml`:
//!
//! ```toml
//! [rules.ts-no-restricted-imports]
//! patterns = ["@banned/*", "lodash"]
//! ```
//!
//! Absent or empty list → rule is a no-op.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-restricted-imports",
    description: "Disallow imports whose module specifier matches a configured pattern list.",
    remediation: "Replace the restricted import with the recommended alternative, or remove the pattern from `[rules.ts-no-restricted-imports] patterns` in `comply.toml`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-restricted-imports"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
