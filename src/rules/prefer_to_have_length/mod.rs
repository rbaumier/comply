//! prefer-to-have-length — suggest `toHaveLength(n)` over `toBe(n)` / `toEqual(n)` on `.length`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-to-have-length",
    description: "Use `toHaveLength(n)` instead of asserting on `.length` with `toBe`/`toEqual`.",
    remediation: "Use expect(x).toHaveLength(n) instead",
    severity: Severity::Warning,
    doc_url: Some("https://jestjs.io/docs/expect#tohavelengthnumber"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
