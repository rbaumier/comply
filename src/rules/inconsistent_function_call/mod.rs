//! inconsistent-function-call — SonarJS S3686.
//!
//! A function must be called consistently: either always with `new`
//! (constructor usage) or always without. Mixing both styles is almost
//! certainly a bug — either a missing `new` (the caller gets `undefined`
//! back on a constructor that sets `this.*`) or a stray `new` on a plain
//! function (which allocates an unused object).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "inconsistent-function-call",
    description: "A function must be called consistently — always with `new` or always without.",
    remediation: "Pick one style per function. If it sets `this.*`, always call with `new`; otherwise, never use `new`.",
    severity: Severity::Error,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S3686"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
